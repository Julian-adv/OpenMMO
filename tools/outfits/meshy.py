"""Reproducible, resumable Meshy generation for modular outfit parts."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
API = 'https://api.meshy.ai/openapi/v1/multi-image-to-3d'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    temporary.replace(path)


def request(suffix='', payload=None):
    key = (Path.home() / '.config/meshy/key').read_text().strip()
    req = urllib.request.Request(
        API + suffix,
        data=json.dumps(payload).encode() if payload is not None else None,
        headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'},
    )
    with urllib.request.urlopen(req, timeout=120) as response:
        return json.load(response)


def prepare(config):
    for part in config['parts']:
        for view in part['views']:
            source, output = ROOT / view['source'], ROOT / view['path']
            if digest(source) != view['source_sha256']:
                raise ValueError(f"Source changed: {view['source']}")
            output.parent.mkdir(parents=True, exist_ok=True)
            if source == output:
                continue
            if 'crop' in view:
                x, y, width, height = view['crop']
                subprocess.run([
                    'ffmpeg', '-nostdin', '-v', 'error', '-y', '-i', str(source),
                    '-vf', f'crop={width}:{height}:{x}:{y}', '-frames:v', '1', str(output),
                ], check=True)
            else:
                output.write_bytes(source.read_bytes())
            print(f"Prepared {view['path']}", flush=True)


def inputs_for(part):
    for view in part['views']:
        if 'source_sha256' in view and digest(ROOT / view['source']) != view['source_sha256']:
            raise ValueError(f"Source changed: {view['source']}")
    return [{'path': view['path'], 'sha256': digest(ROOT / view['path'])}
            for view in part['views']]


def submit(config, record, record_path):
    for part in config['parts']:
        inputs_for(part)
    for part in config['parts']:
        parameters = dict(config['parameters'], target_polycount=part['target_triangles'])
        inputs = inputs_for(part)
        previous = next((task for task in record['tasks'] if task['part_id'] == part['id']), None)
        if previous:
            if previous['inputs'] != inputs or previous['parameters'] != parameters:
                raise ValueError(f"Inputs/settings changed for {part['id']}; use a new versioned record")
            if not previous.get('task_id'):
                raise ValueError(f"Uncertain submission for {part['id']}; reconcile Meshy task history before retrying")
            print(f"Skip existing {part['id']}: {previous['task_id']}", flush=True)
            continue
        task = dict(part_id=part['id'], inputs=inputs, parameters=parameters,
                    status='SUBMITTING', task_id=None)
        record['tasks'].append(task)
        save(record_path, record)
        images = ['data:image/png;base64,' + base64.b64encode((ROOT / v['path']).read_bytes()).decode()
                  for v in part['views']]
        result = request(payload=dict(parameters, image_urls=images))
        task.update(task_id=result['result'], status='SUBMITTED')
        save(record_path, record)
        print(f"Submitted {part['id']}: {task['task_id']}", flush=True)


def download(url, path):
    if path.exists():
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + '.tmp')
    with urllib.request.urlopen(url, timeout=180) as response:
        temporary.write_bytes(response.read())
    temporary.replace(path)


def collect(config, record, record_path):
    for task in record['tasks']:
        if not task.get('task_id'):
            print(f"Uncertain submission: {task['part_id']}", flush=True)
            continue
        result = request('/' + task['task_id'])
        for key in ('status', 'progress', 'consumed_credits', 'created_at', 'finished_at', 'task_error'):
            if key in result:
                task[key] = result[key]
        save(record_path, record)
        print(task['part_id'], task['status'], result.get('progress'), flush=True)
        if task['status'] != 'SUCCEEDED':
            continue
        folder = ROOT / config['output_dir'] / task['part_id']
        files = {'source.glb': result['model_urls']['glb']}
        if result.get('thumbnail_url'):
            files['preview.png'] = result['thumbnail_url']
        for name, url in result.get('thumbnail_urls', {}).items():
            files[f'preview-{name}.png'] = url
        for i, maps in enumerate(result.get('texture_urls', [])):
            for name, url in maps.items():
                if url:
                    files[f'texture-{i}-{name}.png'] = url
        task['files'] = []
        for name, url in files.items():
            path = folder / name
            download(url, path)
            task['files'].append({'path': str(path.relative_to(ROOT)), 'sha256': digest(path)})
        save(record_path, record)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['plan', 'prepare', 'submit', 'collect'])
    parser.add_argument('config', type=Path)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    record_path = ROOT / config['record']
    record = json.loads(record_path.read_text()) if record_path.exists() else dict(
        generation_date=config['generation_date'], service='Meshy Multi-Image to 3D API',
        tier=config['tier'], license_url=config['license_url'],
        endpoint=API, tasks=[], status='Raw generation; not fitted, rigged or game-ready',
        reference=config.get('reference'),
    )
    if args.command == 'plan':
        for part in config['parts']:
            print(part['id'], f"{len(part['views'])} views", f"{part['target_triangles']} triangles")
        print('No API requests. submit creates paid tasks; collect only retrieves existing tasks.')
    elif args.command == 'prepare':
        prepare(config)
    elif args.command == 'submit':
        submit(config, record, record_path)
    else:
        collect(config, record, record_path)


if __name__ == '__main__':
    main()

import argparse
import hashlib
import json
from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1/reference_v2'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--iteration', required=True)
    parser.add_argument('--model', type=Path)
    parser.add_argument('--clay', action='store_true')
    args = parser.parse_args()
    directory = WORK / 'review' / args.iteration
    directory.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path='/home/jake/.cache/ms-playwright/chromium-1208/chrome-linux64/chrome',
                                    headless=True, args=['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'])
        page = browser.new_page(viewport={'width': 1500, 'height': 1500})
        errors = []
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.add_init_script("""window.__THREE_DEVTOOLS__=new EventTarget();window.__THREE_DEVTOOLS__.addEventListener('observe',e=>{const r=e.detail;if(!r.isWebGLRenderer)return;window.r=r;const f=r.render.bind(r);r.render=(s,c)=>{window.s=s;window.c=c;return f(s,c)}})""")
        if args.model:
            page.route('**/__female-preview/face-custom.glb', lambda route: route.fulfill(path=str(args.model.resolve()), content_type='model/gltf-binary'))
        page.goto('http://127.0.0.1:10124/modular-character-preview.html?character=female-face&face=custom&hair=none&camera=face', wait_until='networkidle')
        page.wait_for_selector('body[data-ready="true"]')
        page.evaluate('r.setAnimationLoop(null);r.setPixelRatio(1)')
        if args.clay:
            page.evaluate("""s.traverse(o=>{if(!o.isMesh)return;if(o.name==='eyelashes_female_custom'){o.visible=false;return}const clean=m=>{const a=m.clone();a.map=null;a.normalMap=null;a.roughnessMap=null;a.metalnessMap=null;a.aoMap=null;a.metalness=0;a.roughness=.8;a.color.set('#999999');return a};o.material=Array.isArray(o.material)?o.material.map(clean):clean(o.material)})""")
        views = [
            ('front', [940, 1184], [.001, 1.6821, .813], [.001, 1.6821, 0], 20),
            ('eyes', [1184, 420], [.001, 1.6821, .813], [.001, 1.6821, 0], 3.6),
            ('angle', [900, 900], [.085, 1.695, .155], [.035, 1.685, .064], 35),
            ('other-angle', [900, 900], [-.0866, 1.695, .155], [-.0366, 1.685, .064], 35),
            ('profile', [900, 900], [.45, 1.682, .06], [.033, 1.682, .06], 12),
            ('other-profile', [900, 900], [-.4516, 1.682, .06], [-.0346, 1.682, .06], 12),
            ('head-profile', [900, 1100], [1, 1.67, .01], [0, 1.67, .01], 22),
        ]
        for name, size, position, target, fov in views:
            page.evaluate("""size=>{const h=document.getElementById('viewport');h.style.width=size[0]+'px';h.style.height=size[1]+'px';h.style.minHeight='0';h.style.overflow='visible';h.parentElement.style.gridTemplateColumns=size[0]+'px 310px';h.querySelector('.hint').style.display='none'}""", size)
            page.wait_for_timeout(100)
            page.evaluate("""({size,position,target,fov})=>{
                r.setSize(...size,false);r.domElement.style.width=size[0]+'px';r.domElement.style.height=size[1]+'px';
                c.aspect=size[0]/size[1];c.fov=fov;c.position.fromArray(position);c.lookAt(...target);c.updateProjectionMatrix();
                r.render(s,c);
            }""", {'size': size, 'position': position, 'target': target, 'fov': fov})
            page.locator('#viewport canvas').screenshot(path=str(directory / f'{name}.png'))
        assert not errors, errors
        model = args.model or WORK.parent / 'base_custom_preview.glb'
        record = {'iteration': args.iteration, 'model_sha256': hashlib.sha256(model.read_bytes()).hexdigest(),
                  'render_mode': 'clay' if args.clay else 'material',
                  'browser_errors': errors, 'views': [view[0] for view in views],
                  'camera_parameters': {name: {'viewport': size, 'position': position, 'target': target, 'fov': fov}
                                        for name, size, position, target, fov in views}}
        if not args.model:
            record['build'] = json.loads((WORK / 'build.json').read_text())
        (directory / 'capture.json').write_text(json.dumps(record, indent=2) + '\n')
        browser.close()
        print(json.dumps(record))


if __name__ == '__main__':
    main()

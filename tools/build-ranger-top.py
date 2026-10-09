"""Rebuild the final ranger top without retaining intermediate models."""
import contextlib
import importlib.util
import io
import json
import shutil
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/ranger/tripo_top_v4'


def helper(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'tools' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    fitting = helper('ranger_fitting', 'fit-tripo-ranger-top.py')
    waist = helper('ranger_waist', 'relax-tripo-ranger-waist.py')
    lining = helper('ranger_lining', 'trim-tripo-ranger-lining.py')
    source = OUTPUT / 'source.glb'
    with tempfile.TemporaryDirectory(prefix='.ranger-build-', dir=ROOT / 'assets') as directory:
        stage = Path(directory)
        with contextlib.redirect_stdout(io.StringIO()):
            fitted = fitting.main(source, stage / 'fitted', stage / 'fitted.json')
            relaxed = waist.main(stage / 'fitted/top_ranger.glb', stage / 'relaxed.glb', stage / 'relaxed.json')
            final = lining.main(stage / 'relaxed.glb', stage / 'final.glb', stage / 'final.json')
        assert fitting.fit.digest(stage / 'final.glb') == 'ec8e9638833ad4f60ba66c1a36fb00cd66521c601977b0fa6d7d389ebfaf3623'
        shutil.copyfile(stage / 'final.glb', OUTPUT / 'top_ranger.glb')
        final['source'] = dict(path=str(source.relative_to(ROOT)), sha256=fitting.fit.digest(source))
        final['stages'] = [
            dict(stage='Torso and sleeve fitting', output_sha256=fitted['validation']['sha256'],
                 tightening=fitted['tightening'], skin_weight_policy=fitted['skin_weight_policy']),
            dict(stage='Waist clearance and pelvis motion', output_sha256=relaxed['validation']['sha256'],
                 waist_radial_expansion_m=relaxed['waist_radial_expansion_m'], waist_skinning=relaxed['waist_skinning']),
            dict(stage='Concealed lining removal', output_sha256=final['validation']['sha256']),
        ]
        final['validation'] = fitting.fit.validate(OUTPUT / 'top_ranger.glb')
        final['intermediate_models_retained'] = False
        report = ROOT / 'doc/assets/modular-ranger-tripo-top-fitting-v4.json'
        report.write_text(json.dumps(final, indent=2) + '\n')
    print(json.dumps(final['validation'], indent=2))


if __name__ == '__main__':
    main()

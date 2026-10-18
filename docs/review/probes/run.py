#!/usr/bin/env python3
"""Print diagnostic observations of known Eden defects; this is not a passing test suite.

Requires the normal Rust/SDL2 development prerequisites. Generated wrappers,
executable and synthetic audio live only in a temporary directory.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[3])
args = parser.parse_args()
root = args.repo.resolve()
source = root / 'src'
result = subprocess.run(['cargo', 'build', '--locked', '--message-format=json'],
                        cwd=root, check=True, capture_output=True, text=True)
dependencies = {'sdl2', 'cpal', 'serde', 'serde_json', 'hound', 'lewton', 'midly'}
libraries = {}
for line in result.stdout.splitlines():
    item = json.loads(line)
    if item.get('reason') == 'compiler-artifact' and item['target']['name'] in dependencies:
        for filename in item['filenames']:
            if filename.endswith('.rlib'):
                libraries[item['target']['name']] = filename
missing = dependencies - libraries.keys()
if missing:
    raise SystemExit(f'Missing compiled libraries: {sorted(missing)}')
metadata = json.loads(subprocess.check_output(
    ['cargo', 'metadata', '--no-deps', '--format-version=1'], cwd=root, text=True))
with tempfile.TemporaryDirectory(prefix='eden-review-probes-') as directory:
    work = Path(directory)
    views = (source / 'views/mod.rs').read_text()
    def module(match):
        name = match[1]
        path = work / 'track_lanes_probe.rs' if name == 'track_lanes' else source / 'views' / f'{name}.rs'
        return f'#[path = {json.dumps(str(path))}] mod {name};'
    views = re.sub(r'^mod (\w+);', module, views, flags=re.M)
    (work / 'views_probe.rs').write_text(views + '\npub use track_lanes::probe_drag;\n')
    (work / 'track_lanes_probe.rs').write_text(
        f'include!({json.dumps(str(source / "views/track_lanes.rs"))});\n'
        'pub fn probe_drag(s: &mut AppState, i: &InputState) { handle_clip_drag(s, i, 100.0, 0.0, 0); }\n')
    template = Path(__file__).with_name('probe.rs.in').read_text()
    template = template.replace('@EDEN_ROOT@', json.dumps(str(root))[1:-1])
    template = template.replace('@PROBE_DIR@', json.dumps(str(work))[1:-1])
    (work / 'probe.rs').write_text(template)
    command = ['rustc', '--edition=2021', str(work / 'probe.rs'), '-L',
               f'dependency={metadata["target_directory"]}/debug/deps']
    for dependency, library in sorted(libraries.items()):
        command.extend(['--extern', f'{dependency}={library}'])
    executable = work / 'probe'
    command.extend(['-o', str(executable)])
    subprocess.run(command, check=True)
    subprocess.run([str(executable)], check=True)

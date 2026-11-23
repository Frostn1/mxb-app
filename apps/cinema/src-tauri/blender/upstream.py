"""The bundled Blender scripts and the prototype they come from (MXB Cinema's pipeline,
C:\\dev\\MXB\\claude-work\\cinema), kept in step.

Each bundled script is one of:
  copy   the prototype's file as it is
  patch  the prototype's file with the app's patches applied (upstream.json: pairs of
         [text in the prototype, what the app has instead]); the patches are what lets a
         script read the job's folders rather than the prototype's
  port   rewritten for the app (scene.py, outfit.py, gpu.py); upstream.json holds a hash of the
         prototype version it was ported from, so a newer prototype is noticed and re-ported

  python upstream.py --check   exit 1 and say which file is behind (build.rs runs the same check)
  python upstream.py --write   regenerate every copy/patch script from the prototype
  python upstream.py --ported FILE   record that FILE's port is now up to date with the prototype
"""
import json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.environ.get('CINEMA_PROTOTYPE_DIR', r'C:\dev\MXB\claude-work\cinema')
SPEC = os.path.join(HERE, 'upstream.json')


def read(p):
    with open(p, encoding='utf-8') as f:
        return f.read().replace('\r\n', '\n')


def fnv(text):
    h = 0xcbf29ce484222325
    for b in text.encode('utf-8'):
        h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return f'{h:016x}'


def expected(entry):
    up = read(os.path.join(SRC, entry['from']))
    for old, new in entry.get('patches', []):
        if old not in up:
            raise ValueError(f"{entry['file']}: the prototype no longer has {old[:70]!r}")
        up = up.replace(old, new, 1)
    return up


def main():
    spec = json.load(open(SPEC, encoding='utf-8'))
    if not os.path.isdir(SRC):
        print('upstream: no prototype here, nothing to check')
        return 0
    behind = []
    for e in spec:
        mine = os.path.join(HERE, e['file'])
        if e['kind'] == 'port':
            h = fnv(read(os.path.join(SRC, e['from'])))
            if '--ported' in sys.argv and sys.argv[sys.argv.index('--ported') + 1] == e['file']:
                e['hash'] = h
            elif h != e['hash']:
                behind.append(f"{e['file']}: the prototype's {e['from']} changed since it was ported")
            continue
        try:
            want = expected(e)
        except ValueError as err:
            behind.append(str(err))
            continue
        if '--write' in sys.argv:
            with open(mine, 'w', encoding='utf-8', newline='\n') as f:
                f.write(want)
        elif read(mine) != want:
            behind.append(f"{e['file']}: differs from {e['from']} + its patches")
    if '--ported' in sys.argv:
        with open(SPEC, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(spec, f, indent=1, ensure_ascii=False)
            f.write('\n')
    for b in behind:
        print('upstream:', b)
    return 1 if behind else 0


if __name__ == '__main__':
    sys.exit(main())

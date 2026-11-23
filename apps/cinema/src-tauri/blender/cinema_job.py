"""The job this Blender run is for (job.json, written by the app beside the shot), and where the shot's
textures go. Every bundled script imports it instead of carrying paths of its own."""
import json, os, sys

_argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
JOB_FILE = _argv[_argv.index('--job') + 1] if '--job' in _argv else os.environ.get('CINEMA_JOB_FILE', '')
JOB = json.load(open(JOB_FILE)) if JOB_FILE else {}
SHOT_DIR = os.path.dirname(JOB.get('out_blend', '')) or os.getcwd()
OUT_TEX = os.path.join(SHOT_DIR, 'textures')
os.makedirs(OUT_TEX, exist_ok=True)


def log(*a):
    print('CINEMA', *a, flush=True)

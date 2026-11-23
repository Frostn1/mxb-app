# Runs first in every Blender MXB Cinema starts (-P gpu.py before anything else).
# Background mode does not enable GPU devices by itself, so Cycles silently falls back to the CPU.
# This turns on OptiX (or CUDA) on the NVIDIA card only (an integrated GPU would slow it down).
# A job that renders (CINEMA_NEED_GPU=1) refuses to go on without one; building a .blend doesn't need it.
import os
import bpy

pr = bpy.context.preferences.addons['cycles'].preferences
for t in ('OPTIX', 'CUDA'):
    try:
        pr.compute_device_type = t
        break
    except Exception:
        continue
try:
    pr.get_devices()
except Exception:
    pass
gpus = [d for d in pr.devices if d.type == pr.compute_device_type and 'NVIDIA' in d.name.upper()]
for d in pr.devices:
    d.use = d in gpus
if not gpus and os.environ.get('CINEMA_NEED_GPU') == '1':
    print("CINEMA ERROR noGpu", flush=True)
    raise SystemExit("gpu.py: no NVIDIA GPU found, refusing to render on the CPU")
for sc in bpy.data.scenes:
    sc.cycles.device = 'GPU' if gpus else 'CPU'
print("gpu.py:", pr.compute_device_type, [d.name for d in gpus], flush=True)

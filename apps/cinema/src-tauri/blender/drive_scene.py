"""Drive scene.blend's game-named pivots from CINEMA_bike_root props (keyed by animate.py).
Runs after animate.py in the same Blender session:
  blender -b scene.blend -P animate.py -P drive_scene.py -P render.py -- --json clip.json ...
Swingarm comes from the shock through the .geom linkage (scene.py solves it into the root's `linkage` table),
rider lean from chassis roll (only with --lean-gain > 0; pose.py moves the body otherwise), pivoted at the seat."""
import bpy, json, sys, math, os, argparse
import numpy as np
from mathutils import Matrix, Vector

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
ap = argparse.ArgumentParser()
ap.add_argument('--fork-ref', type=float, default=0.0)   # fork compression the model was built at
ap.add_argument('--sa-ref', type=float, default=0.0)     # authored .geom frame = linkage angle 0 (checked: linkage_check.py)
ap.add_argument('--lean-gain', type=float, default=0.25)
a, _ = ap.parse_known_args(argv)

HERE = os.path.dirname(os.path.abspath(__file__))
sc = bpy.context.scene
O = bpy.data.objects
root = O['CINEMA_bike_root']

# 1) top-level LOD duplicates (edf objects *b / *c) sit at the world origin: keep them out of the render
for o in list(sc.objects):
    eo = o.get('edf_object', '')
    if o.parent is None and isinstance(eo, str) and eo[-1:] in ('b', 'c') and eo[:-1] in ('chassis', 'steer', 'fsusp', 'rsusp', 'rwheel'):
        for ob in [o] + list(o.children_recursive):
            ob.hide_render = True
            ob.hide_viewport = True

# 2) swingarm prop keyed from shock via linkage
tab = json.loads(root.get('linkage', '[]')) or [[0.0, 0.0], [1.0, 0.0]]
xs, ys = [s for s, _ in tab], [r for _, r in tab]
f0, f1 = sc.frame_start, sc.frame_end
fc_shock = root.animation_data.action.fcurves if hasattr(root.animation_data.action, 'fcurves') else None


def prop_at(name, fr):
    sc.frame_set(fr)
    return float(root.get(name, 0.0))


sa_vals = []
for fr in range(f0, f1 + 1):
    sa = float(np.interp(prop_at('shock', fr), xs, ys))
    root['swingarm'] = sa
    root.keyframe_insert('["swingarm"]', frame=fr)
    sa_vals.append(sa)
sa_ref = a.sa_ref if a.sa_ref is not None else float(np.interp(0.0, xs, ys))


def drv(obj, path, idx, expr, **vars_):
    obj.driver_remove(path, idx)
    d = obj.driver_add(path, idx).driver
    d.type = 'SCRIPTED'
    for vn, prop in vars_.items():
        v = d.variables.new()
        v.name = vn
        v.targets[0].id = root
        v.targets[0].data_path = f'["{prop}"]'
    d.expression = expr
    d.is_valid = True


steer, fsusp, rsusp, fwheel, rwheel = (O[n] for n in ('steer', 'fsusp', 'rsusp', 'fwheel', 'rwheel'))
# steer: about its own local Z -> ZYX euler applies Z first (innermost)
sx = steer.rotation_euler.x
steer.rotation_mode = 'ZYX'
steer.rotation_euler = (sx, 0, 0)
drv(steer, 'rotation_euler', 2, 's', s='steer')                  # game m_fSteer: + = LEFT (negative = right); +Z turns the front to Blender +X = bike left
fz = fsusp.location.z
drv(fsusp, 'location', 2, f'{fz:.6f}+f-{a.fork_ref:.6f}', f='fork')   # compression slides the wheel up the axis
# swingarm keyed directly (a driver on this channel came back invalid in -b runs): + = axle up
rsusp.driver_remove('rotation_euler', 0)
rsusp.rotation_mode = 'XYZ'
for k, fr in enumerate(range(f0, f1 + 1)):
    rsusp.rotation_euler.x = sa_vals[k] - sa_ref
    rsusp.keyframe_insert('rotation_euler', index=0, frame=fr)
fwx = fwheel.rotation_euler.x
drv(fwheel, 'rotation_euler', 0, f'{fwx:.6f}+w', w='wheel_rot_f')  # + = rolling forward (-Y)
drv(rwheel, 'rotation_euler', 0, 'w', w='wheel_rot_r')

# 3) rider lean beyond the bike, pivoted at the seat so the pelvis stays on it. Off (--lean-gain 0) whenever pose.py
#    moves the body from the telemetry itself, and there is nothing to lean without a rider.
rr = O.get('CINEMA_rider_root')
if rr is not None and a.lean_gain != 0:
    base = rr.matrix_basis.copy()
    seat = Vector((0.0, 0.05, 0.95))
    if rr.animation_data:
        rr.animation_data_clear()
    rr.rotation_mode = 'XYZ'
    for fr in range(f0, f1 + 1):
        sc.frame_set(fr)
        R = root.matrix_world.to_3x3()
        up, fwd = R @ Vector((0, 0, 1)), R @ Vector((0, -1, 0))
        side = fwd.cross(Vector((0, 0, 1)))
        roll = math.asin(max(-1, min(1, up.dot(side.normalized())))) if side.length > 1e-6 else 0.0
        lean = a.lean_gain * roll
        # rotate about the bike's forward axis (local Y); sign chosen so the rider tilts further the way the bike does
        Rl = Matrix.Rotation(-lean, 4, 'Y')
        T = Matrix.Translation(seat) @ Rl @ Matrix.Translation(-seat)
        rr.matrix_basis = T @ base
        rr.keyframe_insert('location', frame=fr)
        rr.keyframe_insert('rotation_euler', frame=fr)
sc.frame_set(f0)
print('DRIVE ok sa_ref', round(sa_ref, 4), 'sa range', round(min(sa_vals), 3), round(max(sa_vals), 3), 'fork_ref', a.fork_ref)

"""Tight, sharp shots (runs after animate.py + drive_scene.py). Replaces cameras.py's wide cut for the "tight" version.
  --no-rider          hide CINEMA_rider_root and everything under it (and its shadow)
  --mode stills|clip  stills: 3 Cycles PNGs; clip: Cycles PNG frames with cuts chase -> trackside -> detail
Cameras: CAM_chase (3/4 front, moves with the bike), CAM_trackside_low (fixed, low, 85-135 mm),
CAM_detail (front fork/wheel, 85 mm, moves with the bike). Focus locked on the chassis / front wheel."""
import bpy, sys, os, math, argparse
import numpy as np
from mathutils import Vector, Matrix

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
ap = argparse.ArgumentParser()
ap.add_argument('--no-rider', action='store_true')
ap.add_argument('--mode', default='stills')        # none: cameras and settings only (cinema_run.py renders)
ap.add_argument('--stills-out', default='')
ap.add_argument('--prefix', default='norider')
ap.add_argument('--samples', type=int, default=256)
ap.add_argument('--res', type=int, nargs=2, default=[1920, 1080])
ap.add_argument('--only', default='')
ap.add_argument('--out', default='')
ap.add_argument('--name', default='tight')
ap.add_argument('--frames', type=int, nargs=2)
ap.add_argument('--step', type=int, default=1)
ap.add_argument('--engine', default='cycles')
ap.add_argument('--suffix', default='')
a, _ = ap.parse_known_args(argv)

sc = bpy.context.scene
O = bpy.data.objects
root = O['CINEMA_bike_root']
F0, F1 = 1, sc.frame_end
n = F1

# ---- 1. alpha cutouts: a base-colour texture with real transparency drives the BSDF alpha ----------------
fixed = []
_mine = set()
for cn in ('bike', 'rider'):
    if cn in bpy.data.collections:
        for o in bpy.data.collections[cn].all_objects:
            for sl in getattr(o, 'material_slots', []):
                if sl.material:
                    _mine.add(sl.material)
for m in _mine:
    nt = m.node_tree
    if not nt:
        continue
    for nd in nt.nodes:
        if nd.type != 'TEX_IMAGE' or not nd.image or nd.image.channels != 4:
            continue
        to_base = [l for l in nd.outputs['Color'].links if l.to_node.type == 'BSDF_PRINCIPLED' and l.to_socket.name == 'Base Color']
        if not to_base:
            continue
        bs = to_base[0].to_node
        if bs.inputs['Alpha'].is_linked:
            continue
        im = nd.image
        if im.size[0] * im.size[1] > 4096 * 4096:
            continue
        px = np.empty(im.size[0] * im.size[1] * 4, dtype=np.float32)
        im.pixels.foreach_get(px)
        if float((px[3::4] < 0.5).mean()) < 0.02:
            continue
        im.alpha_mode = 'STRAIGHT'
        nt.links.new(nd.outputs['Alpha'], bs.inputs['Alpha'])
        try:
            m.surface_render_method = 'DITHERED'
        except Exception:
            pass
        fixed.append(f'{m.name}:{im.name}')
print('SHOTS alpha fixed', fixed)
# foliage cards: add leaf translucency so the back of a card is lit, not a black seam
leaf = 0
for m in bpy.data.materials:
    nt = m.node_tree
    if not nt or m.get('cinema_leaf'):
        continue
    bs = next((nd for nd in nt.nodes if nd.type == 'BSDF_PRINCIPLED' and nd.inputs['Alpha'].is_linked), None)
    out = next((nd for nd in nt.nodes if nd.type == 'OUTPUT_MATERIAL'), None)
    if not bs or not out or not any(c.name == 'track' for o in bpy.data.objects if o.active_material == m for c in o.users_collection):
        continue
    col = bs.inputs['Base Color'].links[0].from_socket if bs.inputs['Base Color'].is_linked else None
    if col is None:
        continue
    tr = nt.nodes.new('ShaderNodeBsdfTranslucent')
    mix = nt.nodes.new('ShaderNodeMixShader')
    mix.inputs[0].default_value = 0.35
    al = bs.inputs['Alpha'].links[0].from_socket
    nt.links.new(col, tr.inputs['Color'])
    nt.links.new(bs.outputs[0], mix.inputs[1])
    nt.links.new(tr.outputs[0], mix.inputs[2])
    tsp = nt.nodes.new('ShaderNodeBsdfTransparent')
    mix2 = nt.nodes.new('ShaderNodeMixShader')
    nt.links.new(al, mix2.inputs[0])
    nt.links.new(tsp.outputs[0], mix2.inputs[1])
    nt.links.new(mix.outputs[0], mix2.inputs[2])
    nt.links.new(mix2.outputs[0], out.inputs['Surface'])
    m['cinema_leaf'] = 1
    leaf += 1
print('SHOTS leaf materials', leaf)


if a.no_rider:
    rr = O.get('CINEMA_rider_root')
    if rr:
        for ob in [rr] + list(rr.children_recursive):
            ob.hide_render = True

foc = O.get('CINEMA_focus') or bpy.data.objects.new('CINEMA_focus', None)
if not foc.users_collection:
    sc.collection.objects.link(foc)
foc.parent = O['chassis']
foc.matrix_parent_inverse = Matrix.Identity(4)
foc.location = (0, -0.1, 0.6)

# ---- render settings: Cycles, sharp ---------------------------------------------------------------
r = sc.render
r.engine = 'CYCLES'
r.resolution_x, r.resolution_y = a.res
r.resolution_percentage = 100
# motion blur on so spinning wheels read; every camera rides with the bike (exact position as
# its aim point), so the bike itself stays sharp and only the spin and the background smear
r.use_motion_blur = True
r.motion_blur_shutter = 0.1  # wheels only; 0.3 smeared spokes and tread (Sean)
r.use_persistent_data = True
cy = sc.cycles
cy.samples = a.samples
cy.use_denoising = True
for k, v in (('denoiser', 'OPENIMAGEDENOISE'), ('denoising_prefilter', 'ACCURATE'), ('denoising_quality', 'HIGH'),
             ('texture_limit_render', 'OFF')):
    try:
        setattr(cy, k, v)
    except Exception:
        pass
cy.filter_width = 1.0
# the GPU is gpu.py's: OptiX on the NVIDIA card only, set before anything else ran


# ---- helpers ---------------------------------------------------------------------------------------
def ground_z(x, y, top=300.0):
    dg = bpy.context.evaluated_depsgraph_get()
    org = Vector((x, y, top))
    for _ in range(30):
        hit, loc, nrm, idx, ob, mat = sc.ray_cast(dg, org, Vector((0, 0, -1)))
        if not hit:
            return None
        if ob and any(c.name == 'track' for c in ob.users_collection):
            return loc.z
        org = loc - Vector((0, 0, 0.01))
    return None


def clear(cp, tp, margin=1.5):
    dg = bpy.context.evaluated_depsgraph_get()
    d = tp - cp
    org = cp.copy()
    for _ in range(10):
        left = d.length - (org - cp).length - margin
        if left <= 0:
            return True
        hit, loc, nrm, idx, ob, mat = sc.ray_cast(dg, org, d.normalized(), distance=left)
        if not hit:
            return True
        if any(c.name in ('bike', 'rider', 'CINEMA') for c in ob.users_collection):
            org = loc + d.normalized() * 0.02
            continue
        return False
    return True


def smooth(arr, w):
    out = []
    for i in range(len(arr)):
        s = arr[max(0, i - w):i + w + 1]
        out.append(sum(s, Vector()) / len(s))
    return out


# per-frame bike state (index i = frame i+1)
P, FW, FWH = [], [], []
for fr in range(1, n + 1):
    sc.frame_set(fr)
    P.append(foc.matrix_world.translation.copy())
    FWH.append(O['fwheel'].matrix_world.translation.copy())
    R = root.matrix_world.to_3x3()
    f = R @ Vector((0, -1, 0))
    FW.append(Vector((f.x, f.y, 0)).normalized())
fwd = [v.normalized() for v in smooth(FW, 10)]
side = [Vector((-v.y, v.x, 0)) for v in fwd]       # bike's left
Pc = P
fork = []
for fr in range(1, n + 1):
    sc.frame_set(fr)
    fork.append(root.get('fork', 0.0))
apex = int(sc.get('cinema_apex_frame', 128)) - 1
air_take = next((i for i in range(apex, 0, -1) if fork[i] > 0.05), max(0, apex - 50))
land = next((i for i in range(apex, n) if fork[i] > 0.05), min(n - 1, apex + 50))


def new_cam(name, lens, fstop, focus_obj):
    cd = bpy.data.cameras.get(name) or bpy.data.cameras.new(name)
    o = O.get(name) or bpy.data.objects.new(name, cd)
    if o.data is not cd:
        o.data = cd
    if not o.users_collection:
        sc.collection.objects.link(o)
    o.parent = None
    for c in list(o.constraints):
        o.constraints.remove(c)
    for ad in (o.animation_data, cd.animation_data):
        if ad:
            ad.action = None
    cd.lens = lens
    cd.sensor_width = 36
    cd.clip_start = 0.05
    cd.clip_end = 3000
    cd.dof.use_dof = True
    cd.dof.focus_object = focus_obj
    cd.dof.aperture_fstop = fstop
    o.rotation_mode = 'QUATERNION'
    return o


def key_cam(o, i, loc, look):
    o.location = loc
    o.rotation_quaternion = (look - loc).to_track_quat('-Z', 'Y')
    o.keyframe_insert('location', frame=i + 1)
    o.keyframe_insert('rotation_quaternion', frame=i + 1)


# ---- CAM_chase: 3/4 front, ahead-left of the bike, looking back at it ------------------------------
chase = new_cam('CAM_chase', 50, 8.0, foc)
Ps = smooth(P, 6)
for i in range(n):
    loc = Ps[i] + fwd[i] * 3.6 + side[i] * 2.2 + Vector((0, 0, 0.3))
    g = ground_z(loc.x, loc.y, loc.z + 3)
    if g is not None:
        loc.z = max(loc.z, g + 0.6)
    key_cam(chase, i, loc, Pc[i] - fwd[i] * 0.15)

# ---- CAM_trackside_low: fixed, side-on to the bike at the apex, long lens, nothing in front ----------
cut1 = air_take + 2
import time; _t0 = time.time()
up = Vector((0, 0, 1))


def frame_clear(b, i):
    """Bike and the frame around it (+-2.4 m along, +-1.3 m up/down) seen with nothing in between."""
    c = P[i]
    pts = [c] + [c + fwd[i] * dx + up * dz for dx in (-2.4, -1.2, 0, 1.2, 2.4) for dz in (-1.3, 0, 1.3)]
    return all(clear(b, p, margin=0.3 if p is c else 0.0) for p in pts)


apx = P[apex]
best = None
for sgn in (1, -1):
    for d in (11, 13, 15, 17, 20):
        for t in (-3, 0, 3):
            base = apx + side[apex] * (sgn * d) + fwd[apex] * t
            g = ground_z(base.x, base.y)
            if g is None:
                continue
            for hgt in (0.4, 1.2, 2.0, 3.0):
                b = Vector((base.x, base.y, g + hgt))
                if not frame_clear(b, apex):
                    continue
                elev = math.degrees(math.atan2(apx.z - b.z, (apx - b).xy.length))
                around = sum(clear(b, P[k]) for k in range(max(cut1, apex - 30), min(land, apex + 30), 5))
                key = (around, -abs(elev - 6.0) - 0.3 * abs(d - 14))
                if best is None or key > best[0]:
                    best = (key, b, elev)
if best is None:
    best = ((0, 0), apx + side[apex] * 14 - up * 3, 0.0)
tpos = best[1]
print('SHOTS trackside', tuple(round(v, 1) for v in tpos), 'key', best[0], 'elev', round(best[2], 1), 'dist', round((apx - tpos).length, 1))
ts = new_cam('CAM_trackside_low', 100, 5.6, foc)
# bike ~50% of the frame width
lens = [max(85.0, min(200.0, 36.0 * (P[i] - tpos).length / 4.4)) for i in range(n)]
lens = [sum(lens[max(0, i - 10):i + 11]) / len(lens[max(0, i - 10):i + 11]) for i in range(n)]
for i in range(n):
    key_cam(ts, i, tpos, Pc[i])
    ts.data.lens = lens[i]
    ts.data.keyframe_insert('lens', frame=i + 1)
cut2 = land - 3
for i in range(apex, land):
    if not clear(tpos, P[i]) or not clear(tpos, FWH[i]):
        cut2 = max(apex + 5, i - 2)
        break
# start the trackside shot only once the bike is in clear view from it
for i in range(cut1, apex):
    if clear(tpos, P[i]) and clear(tpos, FWH[i]):
        cut1 = i
        break

# ---- CAM_detail: front fork + wheel, 85 mm, rides with the bike --------------------------------------
det = new_cam('CAM_detail', 85, 5.6, O['fwheel'])
FWs = smooth(FWH, 2)
for i in range(n):
    look = FWs[i] + Vector((0, 0, 0.25)) - fwd[i] * 0.15
    loc = FWs[i] + side[i] * 3.3 + fwd[i] * 1.3 + Vector((0, 0, 0.1))
    g = ground_z(loc.x, loc.y, loc.z + 3)
    if g is not None:
        loc.z = max(loc.z, g + 0.25)
    key_cam(det, i, loc, look)

mk = sc.timeline_markers
for m in list(mk):
    mk.remove(m)
for fr, o in ((1, chase), (cut1 + 1, ts), (cut2 + 1, det)):
    m = mk.new(f'cut{fr}', frame=fr)
    m.camera = o
sc.camera = chase
print('SHOTS take', air_take + 1, 'apex', apex + 1, 'land', land + 1, 'cuts', cut1 + 1, cut2 + 1)

if a.engine == 'eevee':
    engines = [e.identifier for e in bpy.types.RenderSettings.bl_rna.properties['engine'].enum_items]
    r.engine = 'BLENDER_EEVEE' if 'BLENDER_EEVEE' in engines else 'BLENDER_EEVEE_NEXT'
    ee = sc.eevee
    for k, v in (('taa_render_samples', max(32, a.samples)), ('use_raytracing', True), ('ray_tracing_method', 'SCREEN'),
                 ('use_shadows', True), ('shadow_resolution_scale', 1.0), ('use_fast_gi', False), ('motion_blur_steps', 4)):
        try:
            setattr(ee, k, v)
        except Exception as e:
            print('eevee', k, e)
    try:
        ee.ray_tracing_options.resolution_scale = '1'
    except Exception:
        pass
else:
    cy.use_adaptive_sampling = True
    cy.adaptive_threshold = 0.02
# Sharpness: only the wheel spin may blur. Every curve in the file steps (CONSTANT) except the two wheel
# spin props, and the shutter opens AT the frame (START), so within the shutter the bike, rider, cameras and
# everything else hold still and only the wheels turn.
_held = 0
for _ad in [getattr(_id, 'animation_data', None) for _id in list(bpy.data.objects) + list(bpy.data.cameras) + list(bpy.data.materials) + list(bpy.data.node_groups)]:
    if not _ad or not _ad.action:
        continue
    _act = _ad.action
    _fcs = list(_act.fcurves) if hasattr(_act, 'fcurves') else [fc for _l in _act.layers for _st in _l.strips for _cb in _st.channelbags for fc in _cb.fcurves]
    for _fc in _fcs:
        if _fc.data_path in ('["wheel_rot_f"]', '["wheel_rot_r"]'):
            for _k in _fc.keyframe_points:
                _k.interpolation = 'LINEAR'
            continue
        for _k in _fc.keyframe_points:
            _k.interpolation = 'CONSTANT'
        _held += 1
r.motion_blur_position = 'START'
r.motion_blur_shutter = 0.1  # wheels only; 0.3 smeared spokes and tread (Sean)
print('SHOTS held curves', _held)
# fixed dicing camera: the displaced dirt is tessellated once, not re-diced for every moving camera,
# so persistent data can keep the BVH between frames
try:
    sc.cycles.dicing_camera = ts
except Exception as e:
    print('dicing', e)
r.image_settings.file_format = 'PNG'
if a.mode == 'none':
    pass
elif a.mode == 'stills':
    os.makedirs(a.stills_out, exist_ok=True)
    stills = (('chase', 40, chase), ('trackside', apex + 1, ts), ('detail', land + 2, det))
    for name, fr, cam in stills:
        if a.only and name not in a.only.split(','):
            continue
        for m in list(mk):
            m.camera = cam
        sc.camera = cam
        sc.frame_set(fr)
        r.filepath = os.path.join(a.stills_out, f'{a.prefix}_{name}{a.suffix}.png')
        _ts = time.time()
        bpy.ops.render.render(write_still=True)
        print('STILLTIME', name, round(time.time() - _ts, 1))
        print('STILL', name, 'frame', fr, r.filepath)
else:
    if a.frames:
        sc.frame_start, sc.frame_end = a.frames
    else:
        sc.frame_start, sc.frame_end = 1, n
    sc.frame_step = a.step
    r.filepath = os.path.join(a.out, 'frames_' + a.name, a.name + '_')
    bpy.ops.render.render(animation=True)
print('RENDER done shots', round(time.time() - _t0))

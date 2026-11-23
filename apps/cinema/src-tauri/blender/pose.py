"""MXB Cinema rider pose: Coach's riderPose/riderMotion, driven per frame from the replay telemetry.

Runs after animate.py + drive_scene.py (+ rider/outfit.py), before shots.py:
  blender -b scene.blend -P animate.py -P drive_scene.py [-P ../rider/outfit.py] -P ../rider/pose.py -P shots.py -- --json clip.json ...
The no-rider path (shots.py --no-rider) never runs this file.

Port of mxb-app packages/shared/src/lib:
  riderPose.ts  - riderFrame axes (up/left/forward off the rig), seat/grip/peg mount read off the bike's own
                  meshes (barGrips: the grip meshes under steer; pegs: the peg mesh under chassis), the "ride"
                  quick move (hunch forward, head back up), reachTheBike (two-bone limbs to grips and pegs,
                  elbows out and up, knees forward and out), joint limits (JOINT_LIMIT / clampTurn).
  riderMotion.ts - MOTION_BONES: per-bone [bend, twist, splay] per unit of leanLR / leanFB / bike roll
                  (ROLL_UNIT 45 deg) / stand, summed then clamped.
Coach applies those as euler turns in each bone's own frame; here bend/splay are stated in the rider's own
axes (bend folds forward about his left axis, splay takes a midline bone to his left), which is what the
comments in riderMotion.ts define them as, and the limbs are solved by IK to the grips and pegs.

Telemetry: the clip's .mxbc was recorded without lean/stance binds (LEAN_BIND / STANCE_BIND confidence 0,
no LEAN records), so Coach itself would only show the roll counter-lean. Cinema fills the unknown inputs from
the bike's own motion: in the air leanFB/leanLR follow the pitch/roll rate the rider's input causes (body
English), on the ground leanFB follows acceleration; stance = standing except slow, hard-leaned corners;
crouch = the legs soaking up fork + shock compression, through a spring so the body lags the bike.
"""
import bpy, sys, json, math, argparse, os
import numpy as np
from mathutils import Matrix, Vector, Quaternion

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
ap = argparse.ArgumentParser()
ap.add_argument('--json', required=True)
ap.add_argument('--pose-static', default='')   # debug: "stand" / "sit" -> no telemetry motion
ap.add_argument('--pose-gain', type=float, default=1.0)
a, _ = ap.parse_known_args(argv)

O = bpy.data.objects
sc = bpy.context.scene
arm, rr, ch, steer = O['rider_rig'], O['CINEMA_rider_root'], O['chassis'], O['steer']
pb = arm.pose.bones
bones = arm.data.bones
DEG = math.pi / 180
SIDES = (('Left', 'L', 1.0), ('Right', 'R', -1.0))   # rider's left = Blender +X in the bike frame

# ---------------------------------------------------------------- 0. undo drive_scene's whole-body lean, rest pose
if rr.animation_data:
    rr.animation_data_clear()
try:                                                       # the Cinema app's job names the bike folder
    from cinema_job import JOB
except Exception:
    JOB = {}
geomf = os.path.join(JOB['bike_dir'], 'bike.geom') if JOB.get('bike_dir') else r'C:/dev/MXB/claude-work/cinema/scene/work/MX2OEM_2023_KTM_250_SX-F.geom'
rpos = (0.0, -0.0062, 0.0273)
for line in open(geomf, errors='ignore'):
    line = line.split(';')[0]
    if line.split('=')[0].strip().lower() == 'rider':
        rpos = [float(x) for x in line.split('=')[1].split(',')]
rr.matrix_basis = Matrix.Translation(Vector((-rpos[0], -rpos[2], rpos[1])))
if arm.animation_data:
    arm.animation_data_clear()
for p in pb:
    p.rotation_mode = 'QUATERNION'
    p.location = (0, 0, 0)
    p.rotation_quaternion = (1, 0, 0, 0)
    p.scale = (1, 1, 1)
sc.frame_set(1)
for p in pb:
    for c in p.constraints:
        c.mute = True
bpy.context.view_layer.update()

A_inv = arm.matrix_world.inverted()
CH2A = A_inv @ ch.matrix_world            # chassis-local -> armature space (a translation: rider root offset)
A2CH = CH2A.inverted()
ST2A = A_inv @ steer.matrix_world         # steer-local -> armature space at frame 1
REST = {b.name: b.matrix_local.copy() for b in bones}
HEAD = {n: m.to_translation() for n, m in REST.items()}


def tail(n):
    return bones[n].tail_local.copy()


# ---------------------------------------------------------------- 1. rider frame (riderFrame) and limb lengths
up = (HEAD['riderRIG_Head'] - HEAD['riderRIG_Pelvis']).normalized()
left = HEAD['riderRIG_LeftHip'] - HEAD['riderRIG_RightHip']
left = (left - up * left.dot(up)).normalized()
fwd = left.cross(up).normalized()
# Coach's facing check: index knuckle ahead of the little finger
kn = HEAD['riderRIG_LeftIndex1'] - HEAD['riderRIG_LeftPink1']
if kn.dot(fwd) < 0:
    fwd = -fwd
thigh = (HEAD['riderRIG_LeftKnee'] - HEAD['riderRIG_LeftHip']).length
shin = (tail('riderRIG_LeftKnee') - HEAD['riderRIG_LeftKnee']).length
uarm = (HEAD['riderRIG_LeftElbow'] - HEAD['riderRIG_LeftShoulder']).length
farm = (HEAD['riderRIG_LeftWrist'] - HEAD['riderRIG_LeftElbow']).length
print('POSE frame up', tuple(round(x, 2) for x in up), 'left', tuple(round(x, 2) for x in left), 'fwd',
      tuple(round(x, 2) for x in fwd), 'thigh', round(thigh, 3), 'shin', round(shin, 3), 'arm', round(uarm + farm, 3))


# ---------------------------------------------------------------- 2. the mount: grips, pegs, seat off the bike's meshes
def verts_in(o, M):
    me = o.data
    co = np.empty(len(me.vertices) * 3, np.float32)
    me.vertices.foreach_get('co', co)
    co = co.reshape(-1, 3)
    W = np.array(M @ o.matrix_world)
    return co @ W[:3, :3].T + W[:3, 3]


def part_faces(o, words=None, exclude=None):
    """(vertex ids, polygons) of `o` for faces whose material (or the object's own name) names any of `words`, and none
    of `exclude`. The app's bike is one mesh per part with the pieces as materials (pegs, grips inside chassis/steer);
    the scene's kit import has them as objects; both are read the same way."""
    nm_ = o.name.lower()
    mats = [(m.name.lower() if m else '') for m in o.data.materials] or ['']
    if exclude and any(k in nm_ for k in exclude):
        mok = np.zeros(len(mats), bool)
    else:
        mok = np.array([(not exclude or not any(k in m_ for k in exclude)) and
                        (not words or any(k in m_ for k in words) or any(k in nm_ for k in words)) for m_ in mats])
    mi = np.zeros(len(o.data.polygons), np.int32)
    if len(o.data.polygons):
        o.data.polygons.foreach_get('material_index', mi)
    keep = mok[np.clip(mi, 0, len(mok) - 1)]
    polys = [tuple(p_.vertices) for p_, k_ in zip(o.data.polygons, keep) if k_]
    ids = np.unique(np.fromiter((v for p_ in polys for v in p_), np.int64)) if polys else np.zeros(0, np.int64)
    return ids, polys


def verts_of(o, M, words=None, exclude=None):
    ids, _ = part_faces(o, words, exclude)
    return verts_in(o, M)[ids] if len(ids) else np.zeros((0, 3))


def under(o, name):
    p = o.parent
    while p:
        if p.name == name:
            return True
        p = p.parent
    return False


M3G = Matrix(((-1, 0, 0), (0, 0, -1), (0, 1, 0)))           # game -> Blender, as everywhere in cinema


def _cfg_block(txt, name):
    i = txt.find(name)
    if i < 0:
        return None
    j = txt.find('{', i)
    d, k = 0, j
    while k < len(txt):
        d += {'{': 1, '}': -1}.get(txt[k], 0)
        if d == 0:
            break
        k += 1
    return txt[j + 1:k]


def _cfg_pos(blk):
    v = {}
    for line in (_cfg_block(blk, 'pos') or '').splitlines():
        line = line.split(';')[0]
        if '=' in line:
            k_, x_ = line.split('=')
            v[k_.strip()] = float(x_)
    return Vector((v['x'], v['y'], v['z']))


def _read(path):
    try:
        return open(path, errors='ignore').read()
    except Exception:
        return ''


def _json(path):
    try:
        return json.load(open(path))
    except Exception:
        return None


# The app writes both gfx.cfg files beside the job: the bike's in bike_dir, the rider's (and its bind pose) in
# rider_dir/body. The prototype paths below are only for the standalone cinema scene on this PC.
BIKE_GFX = _read(os.path.join(JOB['bike_dir'], 'gfx.cfg')) if JOB.get('bike_dir') else ''
if not BIKE_GFX and not JOB:
    import glob
    _c = sorted(glob.glob(os.path.expanduser(r'~/AppData/Local/MXB Cinema/cache/bike/MX2OEM_2023_KTM_250_SX-F-*/gfx.cfg')))
    BIKE_GFX = _read(_c[-1]) if _c else ''
    if not BIKE_GFX:
        _bj = _json(r'C:/dev/MXB/claude-work/cinema/rig/data/bike.json') or {}
        BIKE_GFX = next((t for n_, t in _bj.get('texts', []) if n_.lower().endswith('gfx.cfg')), '')
RIDER_GFX = _read(os.path.join(JOB['rider_dir'], 'body', 'gfx.cfg')) if JOB.get('rider_dir') else ''
if not RIDER_GFX and not JOB:
    RIDER_GFX = (_json(r'C:/dev/MXB/claude-work/cinema/rig/data/rider.json') or {}).get('gfx_cfg', '')
_rj = _json(os.path.join(JOB['rider_dir'], 'body', 'rider.json')) if JOB.get('rider_dir') else None
if _rj is None:
    _rj = _json(r'C:/dev/MXB/claude-work/cinema/scene/work/rider/rider.json')
RBONES = {b['name']: Matrix([b['bind'][0:4], b['bind'][4:8], b['bind'][8:12], b['bind'][12:16]])
          for b in (_rj or {}).get('bones', []) if b.get('bind')}
print('POSE gfx: bike', 'yes' if BIKE_GFX else 'none', '| rider', 'yes' if RIDER_GFX else 'stock values', '| bind pose', len(RBONES), 'bones')
_steer_blk = _cfg_block(BIKE_GFX, chr(10) + 'steer') or BIKE_GFX
GFX_GRIP_EARLY = {}
for _nm, _sh, _s in SIDES:
    _gb = _cfg_block(_steer_blk, ('leftgrip', 'rightgrip')[_sh == 'R'])
    if _gb:
        GFX_GRIP_EARLY[_sh] = M3G @ _cfg_pos(_gb)


bike_meshes = [o for o in bpy.data.collections['bike'].all_objects if o.type == 'MESH' and len(o.data.vertices) and not o.hide_render]
ST_inv = steer.matrix_world.inverted()
grips = {}
gobjs = [o for o in bike_meshes if under(o, 'steer') and ('grip' in o.name.lower() or any(m and 'grip' in m.name.lower() for m in o.data.materials))]
for nm, sh, s in SIDES:
    pts = np.concatenate([v for v in (verts_of(o, ST_inv, ('grip',)) for o in gobjs)]) if gobjs else None
    if pts is not None:
        pts = pts[pts[:, 0] * s > 0.15]
    if pts is None or len(pts) < 10:   # Coach barGrips: widest steer verts, half a grip inboard
        pts = np.concatenate([verts_in(o, ST_inv) for o in bike_meshes if under(o, 'steer')] + [np.array([[s * 0.35, 0.03, 0.25]])])
        wide = np.abs(pts[:, 0]).max()
        pts = pts[pts[:, 0] * s > wide - 0.1]
    inner, outer = np.abs(pts[:, 0]).min(), np.abs(pts[:, 0]).max()
    c = (pts.min(0) + pts.max(0)) / 2
    grips[sh] = Vector((s * (inner + 0.055), c[1], c[2]))        # steer-local, hand centred on the rubber
    if sh in GFX_GRIP_EARLY:
        grips[sh] = GFX_GRIP_EARLY[sh].copy()             # the game's grip point (gfx.cfg) wins over the mesh
pobjs = [o for o in bike_meshes if under(o, 'chassis') and ('peg' in o.name.lower() or any(m and 'peg' in m.name.lower() for m in o.data.materials))]
pegs = {}
for nm, sh, s in SIDES:
    pts = np.concatenate([verts_of(o, ch.matrix_world.inverted(), ('peg',)) for o in pobjs]) if pobjs else np.zeros((0, 3))
    pts = pts[pts[:, 0] * s > 0.08] if len(pts) else pts
    if len(pts) < 10:
        pegs[sh] = Vector((s * 0.19, 0.15, 0.43))
        continue
    lo, hi = pts.min(0), pts.max(0)
    # the step: outer 60% of the peg's span, top surface
    xin, xout = np.abs(pts[:, 0]).min(), np.abs(pts[:, 0]).max()
    pegs[sh] = Vector((s * (xin + 0.6 * (xout - xin)), (lo[1] + hi[1]) / 2, hi[2]))   # chassis-local
seat = Vector((0.0, 0.2124, 0.9464))
for line in open(geomf, errors='ignore'):
    line = line.split(';')[0]
    if line.split('=')[0].strip().lower() == 'seat_height_ref':
        v = [float(x) for x in line.split('=')[1].split(',')]
        seat = Vector((-v[0], -v[2], v[1]))
print('POSE grips(steer)', {k: tuple(round(x, 3) for x in v) for k, v in grips.items()},
      'pegs(chassis)', {k: tuple(round(x, 3) for x in v) for k, v in pegs.items()}, 'seat', tuple(round(x, 3) for x in seat))

# ---------------------------------------------------------------- 3. feet: an ankle bone that keeps the foot flat on the peg
bpy.context.view_layer.objects.active = arm
for o in bpy.context.selected_objects:
    o.select_set(False)
arm.select_set(True)
bpy.ops.object.mode_set(mode='EDIT')
eb = arm.data.edit_bones
for nm, sh, s in SIDES:
    fn = f'cin_foot_{sh}'
    if fn in eb:
        eb.remove(eb[fn])
    e = eb.new(fn)
    e.head = tail(f'riderRIG_{nm}Knee')
    e.tail = e.head + Vector((0, -0.12, 0))
    e.roll = 0
    e.parent = eb[f'riderRIG_{nm}Knee']
    e.use_connect = False
    e.use_deform = True
bpy.ops.object.mode_set(mode='OBJECT')
REST.update({b.name: b.matrix_local.copy() for b in bones if b.name.startswith('cin_')})
HEAD.update({n: m.to_translation() for n, m in REST.items()})


def ensure_empty(name, parent, loc_local):
    e = O.get(name) or bpy.data.objects.new(name, None)
    if not e.users_collection:
        bpy.data.collections['rider'].objects.link(e)
    e.parent = parent
    e.matrix_parent_inverse = Matrix.Identity(4)
    e.empty_display_size = 0.03
    e.location = loc_local
    e.rotation_euler = (0, 0, 0)
    return e


for nm, sh, s in SIDES:
    p = pb[f'cin_foot_{sh}']
    p.rotation_mode = 'QUATERNION'
    for c in list(p.constraints):
        p.constraints.remove(c)
    # rest orientation of the foot bone, held in the bike frame: the boot stays flat on the peg however the shin tilts
    ref = ensure_empty(f'cin_footref_{sh}', ch, Vector((0, 0, 0)))
    ref.rotation_mode = 'QUATERNION'
    ref.rotation_quaternion = (A2CH.to_3x3() @ REST[f'cin_foot_{sh}'].to_3x3()).to_quaternion() @ Quaternion((1, 0, 0), -6 * DEG) @ Quaternion((0, 0, 1), s * 7 * DEG)
    # no constraint: the foot's turn is baked per frame below (BOOT_FLEX). The game has no ankle bone (its boot is
    # rigid on the shin twist); a foot held flat in the bike frame bent the boot up to 64 deg over a 14 cm blend
    # when the shin tilted (whips, feet off the pegs) and squashed the boot and its shaft.

# boots (stock or the outfit's), bone-parented to the shin twist -> skinned: foot to cin_foot, shaft to the shin
sc.frame_set(1)
bpy.context.view_layer.update()
boot_info = {}
boot_feet = {}
for o in list(bpy.data.collections['rider'].all_objects):
    if o.type != 'MESH' or o.parent != arm:
        continue
    if o.parent_type == 'BONE' and o.parent_bone.endswith('KneeTwist'):
        nm = 'Left' if 'Left' in o.parent_bone else 'Right'
    elif o.parent_type == 'OBJECT' and ('cin_foot_L' in o.vertex_groups or 'cin_foot_R' in o.vertex_groups):
        nm = 'Left' if 'cin_foot_L' in o.vertex_groups else 'Right'      # skinned by an earlier run of this file
    else:
        continue
    sh = nm[0]
    mw = o.matrix_world.copy()
    o.parent_type = 'OBJECT'
    o.parent_bone = ''
    o.parent = arm
    o.matrix_parent_inverse = Matrix.Identity(4)
    o.matrix_world = mw
    V = verts_in(o, A_inv)
    ank = tail(f'riderRIG_{nm}Knee')
    w = np.clip((V[:, 2] - (ank.z - 0.02)) / 0.14, 0, 1)
    w = w * w * (3 - 2 * w)
    for g in list(o.vertex_groups):
        o.vertex_groups.remove(g)
    gs = o.vertex_groups.new(name=f'riderRIG_{nm}KneeTwist')
    gf = o.vertex_groups.new(name=f'cin_foot_{sh}')
    for i, wi in enumerate(w):
        if wi > 0:
            gs.add([i], float(wi), 'REPLACE')
        if wi < 1:
            gf.add([i], float(1 - wi), 'REPLACE')
    for m in list(o.modifiers):
        if m.type == 'ARMATURE':
            o.modifiers.remove(m)
    md = o.modifiers.new('rig', 'ARMATURE')
    md.object = arm
    md.use_vertex_groups = True
    bot = V[V[:, 2] < V[:, 2].min() + 0.025]
    heel, toe = bot[:, 1].max(), bot[:, 1].min()           # +Y = back
    ball = Vector((float((V[:, 0].min() + V[:, 0].max()) / 2), float(heel + 0.66 * (toe - heel)), float(V[:, 2].min())))
    boot_info[sh] = ank - ball
    boot_feet[sh] = (V[w < 0.6] - np.array(ank)).astype(np.float64)     # foot-part verts relative to the ankle
    print('POSE boot', o.name, 'skinned; ankle-ball', tuple(round(x, 3) for x in boot_info[sh]))
for nm, sh, s in SIDES:
    if sh not in boot_info:
        boot_info[sh] = Vector((-s * 0.01, 0.14, 0.086))

# ---------------------------------------------------------------- 4. IK targets on the grips and pegs
HAND_DIR = (0.0, -0.90, -0.44)     # chassis-local: the hand's long axis, forward and down over the grip
HAND_NUDGE = (0.0, -0.006, 0.008)  # chassis-local fine offset of the wrist: palm on the rubber, not in it
HAND_IN = 0.010                    # hand slid this far inboard so the fingers end on the rubber, short of the bar end

# the bike's half-width round the boots (chassis-local, 2.5 cm cells, dilated a cell): boots go outboard of it
_pts = []
for o in bike_meshes:
    if under(o, 'steer') or any(k in o.name.lower() for k in ('peg', 'wheel', 'grip')):
        continue
    _pts.append(verts_of(o, ch.matrix_world.inverted(), None, ('peg', 'wheel', 'grip', 'disc', 'sprocket')))
_pts = np.concatenate(_pts)
CELL = 0.025
_k = np.floor(_pts[:, 1:3] / CELL).astype(int)
HW = {}
for (cy, cz), x in zip(map(tuple, _k), np.abs(_pts[:, 0])):
    for dy in (-1, 0, 1):
        for dz in (-1, 0, 1):
            kk = (cy + dy, cz + dz)
            if x > HW.get(kk, 0):
                HW[kk] = x


def halfwidth(y, z):
    return HW.get((int(math.floor(y / CELL)), int(math.floor(z / CELL))), 0.0)


foot_tgt = {}
for nm, sh, s in SIDES:
    ank = pegs[sh] + boot_info[sh] + Vector((0, 0, 0.012))          # chassis-local: ball of the boot on the peg top
    if sh in boot_feet:
        Vf = boot_feet[sh] + np.array(ank)
        need = max(halfwidth(v[1], v[2]) + 0.006 - s * v[0] for v in Vf)
        need = min(max(need, 0.0), 0.03)                   # the ball of the foot never leaves the peg (it is ~5 cm long)
        if need > 0:
            ank = ank + Vector((s * need, 0, 0))
        print('POSE boot', sh, 'outboard of the frame by', round(need, 3), 'peg', tuple(round(x, 3) for x in pegs[sh]))
    foot_tgt[sh] = ank
    ensure_empty(f'ik_foot_{sh}', ch, ank)
    # the hand round the grip (Coach reachTheBike: wrist a hand short of the grip, then the wrist turned to close the
    # hand on it). The hand continues the forearm (no broken wrist); its knuckle line is turned along the bar.
    g = grips[sh]
    S3 = (ST_inv @ ch.matrix_world).to_3x3()          # chassis -> steer-local rotation (rake)
    u0 = S3 @ Vector((-0.35 * s, -0.75, -0.55)).normalized()   # first guess of the forearm line, refined below
    ensure_empty(f'ik_hand_{sh}', steer, g - u0 * 0.075 + S3 @ Vector((0, 0, 0.02)))
    ensure_empty(f'cin_bar_{sh}', steer, g + S3 @ Vector((-s * 0.25, 0, 0)))
    ensure_empty(f'cin_grip_{sh}', steer, g)
    ensure_empty(f'pole_elbow_{sh}', ch, Vector((s * 0.75, 0.3, 1.2)))
    ensure_empty(f'pole_knee_{sh}', ch, Vector((s * 0.3, -1.2, 1.1)))

for nm, sh, s in SIDES:
    for bn, tg, pole in ((f'riderRIG_{nm}Elbow', f'ik_hand_{sh}', f'pole_elbow_{sh}'), (f'riderRIG_{nm}Knee', f'ik_foot_{sh}', f'pole_knee_{sh}')):
        p = pb[bn]
        for c in list(p.constraints):
            p.constraints.remove(c)
        c = p.constraints.new('IK')
        c.target = O[tg]
        c.pole_target = O[pole]
        c.pole_angle = -90 * DEG
        c.chain_count = 2
        c.use_stretch = False
    # The hand holds the grip the way a rider does (and as REFERENCE_good_hands_sean): its orientation is set in the
    # steer frame, not aimed from wherever the forearm arrives. Back of the hand up and forward, the hand's long axis
    # (wrist -> middle knuckle) running forward and down over the rubber, the knuckle line along the bar (index
    # inboard), palm on the top/back of the grip. The wrist is then placed so the game's hand point (rider gfx.cfg
    # lefthand/righthand pos, wrist frame) is on the game's grip point (bike gfx.cfg leftgrip/rightgrip); the arm's IK
    # reaches for that wrist. No aim from the forearm, so no rolled or twisted wrist on hard whips.
    wb = f'riderRIG_{nm}Wrist'
    p = pb[wb]
    for c in list(p.constraints):
        p.constraints.remove(c)
    L3 = ST_inv.to_3x3().normalized() @ arm.matrix_world.to_3x3().normalized()     # armature -> steer-local
    S3h = (ST_inv @ ch.matrix_world).to_3x3().normalized()                          # chassis -> steer-local
    h_r = (L3 @ (HEAD[f'riderRIG_{nm}Middle1'] - HEAD[wb])).normalized()
    k_r = L3 @ (HEAD[f'riderRIG_{nm}Index1'] - HEAD[f'riderRIG_{nm}Pink1'])
    h_t = (S3h @ Vector(HAND_DIR)).normalized()
    k_t = S3h @ Vector((-s, 0.0, 0.0))

    def _frame(h, k):
        k = (k - h * k.dot(h)).normalized()
        return Matrix((h, k, h.cross(k))).transposed()
    Rd = _frame(h_t, k_t) @ _frame(h_r, k_r).transposed()
    ehr = ensure_empty(f'cin_handrot_{sh}', steer, Vector((0, 0, 0)))
    ehr.rotation_mode = 'QUATERNION'
    ehr.rotation_quaternion = (Rd @ L3 @ REST[wb].to_3x3().normalized()).to_quaternion()
    c = p.constraints.new('COPY_ROTATION')
    c.target = ehr
    c.target_space = c.owner_space = 'WORLD'
    hb = _cfg_block(RIDER_GFX, ('lefthand', 'righthand')[sh == 'R'])
    wpos = _cfg_pos(hb) if hb else Vector((0.08, -0.02 * s, 0.02))
    off_a = (M3G @ (RBONES[wb] @ wpos)) - HEAD[wb] if wb in RBONES else (HEAD[f'riderRIG_{nm}Middle1'] - HEAD[wb]) * 0.8
    gp = GFX_GRIP_EARLY.get(sh, grips[sh])
    O[f'ik_hand_{sh}'].location = gp - Rd @ (L3 @ off_a) + S3h @ (Vector(HAND_NUDGE) + Vector((-s * HAND_IN, 0.0, 0.0)))
    print('POSE hand', nm, 'held in the steer frame, long axis', tuple(round(x, 2) for x in HAND_DIR), 'gfx point offset mm',
          round(off_a.length * 1000, 1))
for p in pb:
    for c in p.constraints:
        c.mute = False
arm.data.pose_position = 'POSE'
try:
    arm.pose.ik_solver = 'LEGACY'
except Exception:
    pass


# ---------------------------------------------------------------- 5. fingers wrapped round the grip
def curl_sign(nm, bn):
    """+1/-1: which turn about the bone's own X brings the finger towards the palm (palm faces the thigh at rest)."""
    m = REST[bn].to_3x3()
    palm = (HEAD['riderRIG_Pelvis'] - HEAD[bn])
    palm = Vector((palm.x, 0, 0)).normalized()
    y = m.col[1]
    for sg in (1, -1):
        r = m @ Matrix.Rotation(sg * 0.5, 3, 'X') @ Vector((0, 1, 0))
        if (r - y).dot(palm) > 0:
            return sg
    return 1


CURL = {'Index': (42, 62, 35), 'Middle': (46, 64, 35), 'Ring': (50, 64, 35), 'Pink': (54, 62, 35), 'Thumb': (25, 35, 25)}   # round a 32 mm grip, not a fist
for nm, sh, s in SIDES:
    for fn, angs in CURL.items():
        for j in (1, 2, 3):
            bn = f'riderRIG_{nm}{fn}{j}'
            if bn not in pb:
                continue
            sg = curl_sign(nm, bn)
            pb[bn].rotation_quaternion = Quaternion((1, 0, 0), sg * angs[j - 1] * DEG)


# ---------------------------------------------------------------- 6. telemetry -> motion (riderMotion inputs)
D = json.load(open(a.json))
F = D['riders'][0]['frames']
Mg = np.array(D['to_blender'], float)
n = len(F)
# The game's own skeleton, when the recording has it (mxbcoach RIDER_BONES, mxbc2json frames[].bones): every
# riderRIG_* bone but the fingers, as the game drew it, rotation + position in the bike body's frame (game axes).
# Then the rider is posed straight from it: no guessed motion, no IK to grips/pegs, no per-shot override.
SKEL = D['riders'][0].get('skeleton') or {}
_bf = [i for i, f in enumerate(F) if f.get('bones')]
USE_BONES = SKEL.get('status') == 'ok' and len(_bf) > 0 and os.environ.get('CIN_BONES', '1') != '0'
print('POSE bones:', ('recorded skeleton on %d/%d frames' % (len(_bf), n)) if USE_BONES else
      ('none (%s): guessed pose' % ('CIN_BONES=0' if _bf and SKEL.get('status') == 'ok' else (SKEL.get('status') or 'not in the recording'))))
dt = 1.0 / D.get('fps', 50)
pitch, roll, speed, air, comp = [], [], [], [], []
for f in F:
    R = Mg @ np.array(f['rot']) @ Mg.T
    fw, rt = R @ [0, -1, 0], R @ [-1, 0, 0]
    pitch.append(math.degrees(math.asin(max(-1, min(1, fw[2])))))       # nose up +
    roll.append(math.degrees(math.asin(max(-1, min(1, -rt[2])))))       # to the rider's right +
    speed.append(f.get('speed') or 0.0)
    air.append(bool(f.get('airborne')))
    comp.append((f.get('fork') or 0.0, f.get('shock') or 0.0))
pitch, roll, speed = np.array(pitch), np.array(roll), np.array(speed)


def smooth(x, w):
    k = np.ones(2 * w + 1) / (2 * w + 1)
    return np.convolve(np.pad(x, w, mode='edge'), k, mode='valid')


def rate(x, w=4):
    xs = smooth(x, w)
    return np.gradient(xs, dt)


prate, rrate, acc = rate(pitch), rate(roll), rate(speed, 6)
airs = smooth(np.array(air, float), 3)
# leanFB / leanLR in -1..1 (riderMotion leanUnit); air: the input that made the bike rotate, ground: accel
lean_fb = np.where(airs > 0.5, np.clip(-prate / 55.0, -1, 1), np.clip(acc / 7.0, -1, 1) * 0.6)
lean_lr = np.where(airs > 0.5, np.clip(rrate / 45.0, -1, 1), np.clip(rrate / 60.0, -1, 1) * 0.5)
lean_fb, lean_lr = smooth(lean_fb, 5), smooth(lean_lr, 5)
stand_raw = np.array([0.0 if (not air[i] and abs(roll[i]) > 28 and speed[i] < 13) else 1.0 for i in range(n)])
stand = smooth(stand_raw, 12)
crouch_raw = np.array([0.0 if air[i] else float(np.clip(0.55 * max(0, c[0] - 0.02) + 0.7 * max(0, c[1] - 0.01), 0, 0.15)) for i, c in enumerate(comp)])
if a.pose_static:
    lean_fb[:] = 0; lean_lr[:] = 0; crouch_raw[:] = 0; pitch[:] = 0; roll[:] = 0
    stand[:] = 1.0 if a.pose_static == 'stand' else 0.0


def spring(x, w=11.0, z=0.75):
    """critically-ish damped follower: the body lags and overshoots the bike a little"""
    y, v, out = x[0], 0.0, []
    for t in x:
        acc_ = w * w * (t - y) - 2 * z * w * v
        v += acc_ * dt
        y += v * dt
        out.append(y)
    return np.array(out)


crouch = np.clip(spring(crouch_raw), 0, 0.17)
fbs = spring(lean_fb, 9.0, 0.8)
lrs = spring(lean_lr, 9.0, 0.8)
pitch_s = spring(pitch, 8.0, 0.9)
ground_s = np.clip(spring(1.0 - np.array(air, float), 6.0, 1.0), 0, 1)
roll_u = np.clip(spring(roll, 8.0, 0.9) / 45.0, -1, 1)      # riderMotion ROLL_UNIT

# riderMotion.MOTION_BONES (bend, splay per unit; twist unused there) + the "ride" quick move as the rest it adds to
MOTION = {
    'riderRIG_Spine1': dict(fb=(7, 0), lr=(0, -5), roll=(0, 7)),
    'riderRIG_Spine2': dict(fb=(6, 0), lr=(0, -4), roll=(0, 6)),
    'riderRIG_Neck1': dict(fb=(-4, 0), lr=(0, 2), roll=(0, 4)),
    'riderRIG_Pelvis': dict(fb=(4, 0), stand=(-6, 0)),
    'riderRIG_LeftCollar': dict(fb=(6, 0)),
    'riderRIG_RightCollar': dict(fb=(6, 0)),
}
LIMIT = lambda b: 45 if ('Neck' in b or 'Head' in b) else 70 if ('Collar' in b or 'Wrist' in b) else 60
GAIN = 1.3 * a.pose_gain     # Coach's angles are sized to read at replay thumbnail size; a full-frame still wants more


def base_bend(st):
    """the ride hunch: sitting (st=0) vs standing attack (st=1), degrees of forward bend per bone"""
    sit = {'riderRIG_Pelvis': 12, 'riderRIG_Spine1': 8, 'riderRIG_Spine2': 8, 'riderRIG_Spine3': 6, 'riderRIG_Spine4': 2,
           'riderRIG_Neck1': -10, 'riderRIG_Neck2': -8}
    std = {'riderRIG_Pelvis': 38, 'riderRIG_Spine1': 9, 'riderRIG_Spine2': 8, 'riderRIG_Spine3': 6, 'riderRIG_Spine4': 2,
           'riderRIG_Neck1': -18, 'riderRIG_Neck2': -12}
    return {k: sit[k] * (1 - st) + std[k] * st for k in sit}


FK = ['riderRIG_Pelvis', 'riderRIG_Spine1', 'riderRIG_Spine2', 'riderRIG_Spine3', 'riderRIG_Spine4', 'riderRIG_Neck1',
      'riderRIG_Neck2', 'riderRIG_Head', 'riderRIG_LeftCollar', 'riderRIG_RightCollar']
PAR = {b.name: (b.parent.name if b.parent else None) for b in bones}

# Per-shot pose override (measured, not a rule): when a shot has a reference image (e.g. the game's F10 view), the pose
# for that moment is fitted to the joints marked in the image (fit_pose_f10.py) and saved as a JSON beside the job
# (job "pose_override", or env CIN_POSE_OVERRIDE). It applies only to that shot's frames (full weight on "frames",
# blended in/out over "ramp" frames) and nothing of it reaches any other shot:
#   pelvis_offset [x,y,z] m, armature axes (+X rider's left, -Y forward, +Z up), added to the pelvis place
#   turns {bone: [bend, splay, twist] deg}, added to that bone's turn after the joint limits
#   foot {L/R: [x,y,z]} chassis-local ankle (IK) target, knee_pole {L/R: [x,y,z]} chassis-local knee pole
POSE_OVR_FILE = '' if USE_BONES else (JOB.get('pose_override') or os.environ.get('CIN_POSE_OVERRIDE', ''))
POSE_OVR = _json(POSE_OVR_FILE) if POSE_OVR_FILE else None
if POSE_OVR_FILE:
    print('POSE override', POSE_OVR_FILE, 'frames', (POSE_OVR or {}).get('frames'), 'ramp', (POSE_OVR or {}).get('ramp'))


def ovr_w(i):
    """weight of the per-shot override at frame index i (frame i + 1)"""
    if not POSE_OVR or not POSE_OVR.get('frames'):
        return 0.0
    fr0, fr1 = min(POSE_OVR['frames']), max(POSE_OVR['frames'])
    fr, ramp = i + 1, float(POSE_OVR.get('ramp', 0))
    d = max(fr0 - fr, fr - fr1, 0)
    if d == 0:
        return 1.0
    if d >= ramp:
        return 0.0
    t = 1.0 - d / ramp
    return t * t * (3 - 2 * t)


def foot_at(sh, i):
    """chassis-local ankle target at frame index i: on the peg, or where the shot's override puts it"""
    w = ovr_w(i)
    f = (POSE_OVR or {}).get('foot', {}).get(sh) if w > 0 else None
    return foot_tgt[sh].lerp(Vector(f), w) if f else foot_tgt[sh].copy()


# pelvis targets in armature space
ank_a = {sh: CH2A @ foot_tgt[sh] for sh in ('L', 'R')}
ank_mid = (ank_a['L'] + ank_a['R']) / 2
hip_half = (HEAD['riderRIG_LeftHip'] - HEAD['riderRIG_RightHip']).length / 2
leg = thigh + shin
stand_h = math.sqrt(max(0.01, (0.80 * leg) ** 2 - (abs(ank_a['L'].x) - hip_half) ** 2))
P_STAND = Vector((0, ank_mid.y + 0.10, ank_mid.z + stand_h)) + (HEAD['riderRIG_Pelvis'] - (HEAD['riderRIG_LeftHip'] + HEAD['riderRIG_RightHip']) / 2)
seat_a = CH2A @ seat
pel_bot = HEAD['riderRIG_Pelvis'].z - 0.115          # Coach seatContact: bottom of the pelvis box (rest body 0.77 m)
steer_head_a = CH2A @ (ch.matrix_world.inverted() @ steer.matrix_world.translation)
P_SIT = Vector((0, seat_a.y + (steer_head_a.y - seat_a.y) * 0.25, seat_a.z - 0.02 + (HEAD['riderRIG_Pelvis'].z - pel_bot)))
print('POSE pelvis stand', tuple(round(x, 3) for x in P_STAND), 'sit', tuple(round(x, 3) for x in P_SIT))


def rot_about(pt, R3):
    return Matrix.Translation(pt) @ R3.to_4x4() @ Matrix.Translation(-pt)


def turn(bend, splay, twist=0.0):
    return (Matrix.Rotation(bend * DEG, 3, 'X') @ Matrix.Rotation(splay * DEG, 3, 'Y') @ Matrix.Rotation(twist * DEG, 3, 'Z'))


def pose_frame(i, k=1.0, dz=0.0):
    st = float(stand[i])
    fb, lr, ru = float(fbs[i]) * GAIN * k, float(lrs[i]) * GAIN * k, float(roll_u[i])
    bend = base_bend(st)
    # gravity: the torso stays nearer upright in the world than the bike does (nose up -> fold forward over it)
    bend['riderRIG_Pelvis'] += 0.35 * float(pitch_s[i]) * float(ground_s[i]) * k   # weightless in the air: no gravity lean
    turns = {}
    for b in FK:
        t = [bend.get(b, 0.0), 0.0]
        m = MOTION.get(b, {})
        for k, u in (('fb', fb), ('lr', lr), ('roll', ru * GAIN), ('stand', st)):
            if k in m:
                t[0] += m[k][0] * u
                t[1] += m[k][1] * u
        turns[b] = t
    # head level with the horizon: the whole chain's forward bend minus the bike's nose-up pitch is where the eyes
    # point in the world; aim them 12 deg below the horizon on the ground, 18 in the air (towards the landing)
    look = 12.0 + 6.0 * (1.0 - float(ground_s[i]))
    tot = sum(turns[b][0] for b in FK[:7])
    turns['riderRIG_Head'][0] = look + float(pitch_s[i]) - tot
    # neck extension capped: neck + head never bend back more than 40 deg together, nor forward more than 35
    nb = ('riderRIG_Neck1', 'riderRIG_Neck2', 'riderRIG_Head')
    ext = sum(turns[b][0] for b in nb)
    if ext < -40 or ext > 35:
        lo_hi = -40 if ext < -40 else 35
        turns['riderRIG_Head'][0] += lo_hi - ext
    # clavicles: arms forward and out to the bars lift the shoulder girdle a little (keeps the deltoid on the torso);
    # the side the counter-lean drops gets a little more so the shoulder line stays near level
    tilt = sum(turns[b][1] for b in ('riderRIG_Spine1', 'riderRIG_Spine2', 'riderRIG_Spine3', 'riderRIG_Spine4'))
    turns['riderRIG_LeftCollar'][1] += -(10.0 + max(0.0, tilt) * 0.6)
    turns['riderRIG_RightCollar'][1] += (10.0 + max(0.0, -tilt) * 0.6)
    for b in turns:
        lim = LIMIT(b)
        turns[b] = [max(-lim, min(lim, x)) for x in turns[b]]
    yaw = {}
    wo = ovr_w(i)
    if wo > 0:                                            # this shot's measured pose (see POSE_OVR above)
        for b, (ob, os_, ot) in POSE_OVR.get('turns', {}).items():
            if b in turns:
                turns[b] = [turns[b][0] + wo * ob, turns[b][1] + wo * os_]
                yaw[b] = wo * ot
    # pelvis place: stand/sit blend, weight fore/aft with leanFB, legs soaking up compression
    P = P_SIT.lerp(P_STAND, st)
    P = P + Vector((-0.03 * lr / GAIN, -0.085 * fb / GAIN + 0.06 * crouch[i], -crouch[i] + dz))
    if wo > 0:
        P = P + Vector(POSE_OVR.get('pelvis_offset', (0, 0, 0))) * wo
    # keep the ankles reachable
    for sh, sgn in (('L', 1), ('R', -1)):
        hip = P + Vector((sgn * hip_half, 0, -0.023))
        d = hip - CH2A @ foot_at(sh, i)
        if d.length > 0.95 * leg:
            P = P - d.normalized() * (d.length - 0.95 * leg)
    out = {}
    pose = {}
    for b in FK:
        rest = REST[b]
        par = PAR[b]
        if par is None:
            inh = Matrix.Translation(P - HEAD[b]) @ rest
        else:
            inh = pose[par] @ REST[par].inverted() @ rest
        tb, ts = turns[b]
        R3 = turn(tb, ts, yaw.get(b, 0.0))
        pm = rot_about(inh.to_translation(), R3) @ inh
        pose[b] = pm
        basis = (REST[par].inverted() @ rest).inverted() @ pose[par].inverted() @ pm if par else rest.inverted() @ pm
        out[b] = basis
    return out, pose


# make sure the body rides inside the gear instead of through it, then key every frame
body = O.get('rider_body')
for o in [body] + [x for x in bpy.data.collections['rider'].all_objects if x.type == 'MESH' and any(m.type == 'ARMATURE' for m in x.modifiers)]:
    if o is None or o.type != 'MESH':
        continue
    if o == body:
        for m in o.modifiers:
            if m.type == 'ARMATURE':
                m.use_deform_preserve_volume = True       # dual-quaternion: shoulders/elbows keep their volume
    if o == body and not o.get('cin_welded'):
        # the EDF body has its UV seams as split vertices (same place, own UV): weld them in the mesh itself, before
        # anything indexes it, so skin, smoothing and the glove fix see one surface (no cracks at neck or shoulder).
        # UVs and split normals are per loop and survive the merge.
        import bmesh
        bm_ = bmesh.new(); bm_.from_mesh(o.data)
        n0 = len(bm_.verts)
        bmesh.ops.remove_doubles(bm_, verts=bm_.verts, dist=1e-5)
        bm_.to_mesh(o.data); bm_.free(); o.data.update()
        o['cin_welded'] = 1
        print('POSE body seams welded', n0, '->', len(o.data.vertices), 'vertices')
    if not any(m.type == 'CORRECTIVE_SMOOTH' for m in o.modifiers) and o == body:
        cs = o.modifiers.new('cin_smooth', 'CORRECTIVE_SMOOTH')
        cs.factor = 0.5
        cs.iterations = 6
        cs.smooth_type = 'LENGTH_WEIGHTED'
        cs.rest_source = 'ORCO'
        cs.use_only_smooth = False

# ---------------------------------------------------------------- 7. no body through the bike: seat floor, bar-pad keep-out
def vg_verts(o, bone, wmin=0.5):
    if o is None or bone not in o.vertex_groups:
        return np.zeros((0, 3))
    gi = o.vertex_groups[bone].index
    V = verts_in(o, A_inv)
    ids = [v.index for v in o.data.vertices if any(g.group == gi and g.weight >= wmin for g in v.groups)]
    return V[ids]


def bone_child_verts(bone):
    out = []
    for o in bpy.data.collections['rider'].all_objects:
        if o.type == 'MESH' and o.parent == arm and o.parent_type == 'BONE' and o.parent_bone == bone and not o.hide_render:
            out.append(verts_in(o, A_inv))
    return np.concatenate(out) if out else np.zeros((0, 3))


BUTT = vg_verts(body, 'riderRIG_Pelvis', 0.5)
BUTT = BUTT[BUTT[:, 2] < HEAD['riderRIG_Pelvis'].z] if len(BUTT) else BUTT
HEADPTS = {b: np.concatenate([vg_verts(body, b, 0.5), bone_child_verts(b)]) for b in ('riderRIG_Spine4', 'riderRIG_Neck1', 'riderRIG_Neck2', 'riderRIG_Head')}
TOP = {}
TOPX = {}
for o in bike_meshes:
    if under(o, 'steer') or any(k in o.name.lower() for k in ('peg', 'wheel', 'grip')):
        continue
    V = verts_of(o, A_inv, None, ('peg', 'wheel', 'grip', 'disc', 'sprocket'))   # armature space at frame 1
    for y, x, z in zip(np.floor(V[:, 1] / CELL).astype(int), np.floor(np.abs(V[:, 0]) / CELL).astype(int), V[:, 2]):
        if z > TOPX.get((y, x), -9):
            TOPX[(y, x)] = z
    V = V[np.abs(V[:, 0]) < 0.14]
    for y, z in zip(np.floor(V[:, 1] / CELL).astype(int), V[:, 2]):
        for dy in (-1, 0, 1):
            if z > TOP.get(y + dy, -9):
                TOP[y + dy] = z
_bar = []
for o in bike_meshes:
    if under(o, 'steer') and not any(k in o.name.lower() for k in ('grip', 'fender', 'fork', 'wheel', 'wp_fsusp', 'lowerfork', 'xtrig hilo switch')):
        V = verts_of(o, A_inv, None, ('grip', 'wheel', 'disc'))
        V = V[(np.abs(V[:, 0]) < 0.13) & (V[:, 2] > 1.05)] if len(V) else V
        if len(V):
            _bar.append(V)
_bar = np.concatenate(_bar) if _bar else np.zeros((0, 3))
BAR_LO = _bar.min(0) - 0.03 if len(_bar) else None
BAR_HI = _bar.max(0) + 0.03 if len(_bar) else None
print('POSE keep-out: butt pts', len(BUTT), 'head pts', sum(len(v) for v in HEADPTS.values()), 'bar box',
      None if BAR_LO is None else (tuple(round(x, 2) for x in BAR_LO), tuple(round(x, 2) for x in BAR_HI)))


def xf(M, pts):
    if not len(pts):
        return pts
    W = np.array(M)
    return pts @ W[:3, :3].T + W[:3, 3]


def mid_joint(root, target, near, far, pole):
    """Coach riderPose.midJoint: the knee/elbow of a two-bone limb whose end is put on target, bent towards pole."""
    axis = target - root
    d = axis.length
    if d < 1e-6:
        return root + pole.normalized() * near
    d = min(max(d, abs(near - far) + 1e-4), near + far - 1e-4)
    axis = axis.normalized()
    along = (near * near - far * far + d * d) / (2 * d)
    out = math.sqrt(max(0.0, near * near - along * along))
    bend = pole - axis * pole.dot(axis)
    if bend.length < 1e-8:
        return root + axis * along
    return root + axis * along + bend.normalized() * out


THIGH_R = 0.085
SHIN_R = 0.06


KNEE_POLE = {sh: O[f'pole_knee_{sh}'].location.copy() for _, sh, _ in SIDES}


def knee_pole(sh, kx):
    """knee pole (chassis-local) pushed kx m further out: knees in on the shrouds, spread only as far as the plastics need"""
    s = 1.0 if sh == 'L' else -1.0
    return KNEE_POLE[sh] + Vector((s * kx, 0, 0))


def thigh_need(pose, kx=0.0):
    """how far the hips must come up so neither thigh (hip->knee, predicted the way the IK will solve it) is in the seat/tank"""
    worst = 0.0
    Mp = pose['riderRIG_Pelvis'] @ REST['riderRIG_Pelvis'].inverted()
    for nm, sh, s in SIDES:
        H = Mp @ HEAD[f'riderRIG_{nm}Hip']
        Pk = CH2A @ knee_pole(sh, kx)
        K = mid_joint(H, ank_a[sh], thigh, shin, Pk - H)
        for t in (0.2, 0.35, 0.5, 0.65):
            c = H.lerp(K, t)
            cy = int(math.floor((c.y - CH2A.translation.y) / CELL)); cz = int(math.floor((c.z - CH2A.translation.z) / CELL))
            # the thigh as a 7 cm tube over the bike's height field (seat, tank) at that length along the bike
            cy = int(math.floor(c.y / CELL))
            ax = abs(c.x)
            for cx in range(int((ax - THIGH_R) / CELL), int((ax + THIGH_R) / CELL) + 1):
                top = max(TOPX.get((cy + dy, cx), -9) for dy in (-1, 0, 1))
                dx = min(abs((cx + 0.5) * CELL - ax), THIGH_R)
                worst = max(worst, top + math.sqrt(THIGH_R ** 2 - dx ** 2) - c.z)
    return min(worst, 0.04)


def shin_hit(pose, kx=0.0):
    """knee/shin (predicted like the IK) inside the bike's half-width (side panels, shrouds)?"""
    Mp = pose['riderRIG_Pelvis'] @ REST['riderRIG_Pelvis'].inverted()
    for nm, sh, s in SIDES:
        H = Mp @ HEAD[f'riderRIG_{nm}Hip']
        K = mid_joint(H, ank_a[sh], thigh, shin, CH2A @ knee_pole(sh, kx) - H)
        for c in [H.lerp(K, t) for t in (0.8, 1.0)] + [K.lerp(ank_a[sh], t) for t in (0.15, 0.3, 0.45)]:
            cy = int(math.floor((c.y - CH2A.translation.y) / CELL)); cz = int(math.floor((c.z - CH2A.translation.z) / CELL))
            if HW.get((cy, cz), 0.0) + SHIN_R > s * c.x + 0.01:
                return True
    return False


def solve_frame(i):
    """legs first: spread the knees only as far as the seat/shrouds need; then body English back off from the bars and
    plastics; the hips come up only as the last resort (that is what straightened the legs)"""
    k, dz, kx = 1.0, 0.0, 0.0
    for _ in range(24):
        out, pose = pose_frame(i, k, dz)
        hit = False
        if BAR_LO is not None:
            for b, pts in HEADPTS.items():
                q = xf(pose[b] @ REST[b].inverted(), pts)
                if len(q) and np.any(np.all((q > BAR_LO) & (q < BAR_HI), axis=1)):
                    hit = True
                    break
        if not hit and shin_hit(pose, kx):
            if kx < 0.3:
                kx += 0.05
                continue
            hit = k > 0.3
        if hit and k > 0.2:
            k *= 0.75
            continue
        q = xf(pose['riderRIG_Pelvis'] @ REST['riderRIG_Pelvis'].inverted(), BUTT)
        need = max([TOP.get(int(math.floor(y / CELL)), -9) + 0.015 - z for y, z in zip(q[:, 1], q[:, 2])] + [0.0]) if len(q) else 0.0
        tn = thigh_need(pose, kx)
        if tn > 1e-3 and kx < 0.3:
            kx += 0.05
            continue
        need = max(need, tn)
        if need > 1e-3 and dz < 0.12:
            dz += need
            continue
        break
    return out, k, dz, kx


stats = []


def key_from_bones():
    """Key every recorded bone from the game's skeleton. A recorded bone's skin transform in the bike frame is
    N(t) x bind^-1 (N: the node's recorded matrix, bind: rider.edf's bind pose, both game axes); in armature space
    that is A^-1 x chassis x G(N bind^-1) with G the game -> Blender axes change, and the pose bone's armature-space
    matrix is that times its rest matrix. Bones the game does not record (fingers, cin_*) keep their own basis."""
    G4 = Matrix(Mg.tolist()).to_4x4()
    CH2Af = A_inv @ ch.matrix_world                   # chassis -> armature (constant: the rider rides the chassis)
    RB_inv = {b: m.inverted() for b, m in RBONES.items()}
    have = [k for k in _bf]
    missing = 0
    order = [b.name for b in bones]                   # parents before children
    for p_ in pb:
        for c in list(p_.constraints):                # the game placed hands and feet: no IK, no hand aim
            if p_.name.startswith('riderRIG_'):
                p_.constraints.remove(c)
    for i in range(n):
        fr = i + 1
        k = i if F[i].get('bones') else min(have, key=lambda j: abs(j - i))
        missing += k != i
        BN = F[k]['bones']
        P = {}
        for b in order:
            par = PAR[b]
            rec = BN.get(b) if b in RBONES else None
            if rec:
                qw, qx, qy, qz, x, y, z = rec
                N_ = Quaternion((qw, qx, qy, qz)).normalized().to_matrix().to_4x4()
                N_.translation = Vector((x, y, z))
                P[b] = CH2Af @ (G4 @ (N_ @ RB_inv[b]) @ G4.transposed()) @ REST[b]
            else:
                bas = pb[b].matrix_basis.copy()
                P[b] = (P[par] @ REST[par].inverted() @ REST[b] if par else REST[b]) @ bas
            if not rec:
                continue
            bas = ((REST[par].inverted() @ REST[b]).inverted() @ P[par].inverted() @ P[b]) if par else REST[b].inverted() @ P[b]
            loc, q, _ = bas.decompose()
            p_ = pb[b]
            p_.rotation_quaternion = q
            p_.keyframe_insert('rotation_quaternion', frame=fr)
            if not bones[b].use_connect:
                p_.location = loc
                p_.keyframe_insert('location', frame=fr)
    print('POSE bones keyed', n, 'frames,', sum(1 for b in order if b in RBONES and b in F[_bf[0]]['bones']), 'bones from the game,',
          missing, 'frames held from the nearest recorded one;', 'not in rider.json bind:', [b for b in F[_bf[0]]['bones'] if b not in RBONES])


for i in range(0 if USE_BONES else n):
    fr = i + 1
    wo = ovr_w(i)
    if wo > 0:
        # the shot's measured pose: the joints were fitted to the image, so no body-English back-off, no seat lift, no
        # knee spread on top of it (those would move the joints off the marks)
        bas, _ = pose_frame(i, 1.0, 0.0)
        kk, dzz, kxx = 1.0, 0.0, 0.0
    else:
        bas, kk, dzz, kxx = solve_frame(i)
    stats.append((kk, dzz))
    for _, sh, _ in SIDES:
        kp_ = knee_pole(sh, kxx)
        po = (POSE_OVR or {}).get('knee_pole', {}).get(sh) if wo > 0 else None
        O[f'pole_knee_{sh}'].location = kp_.lerp(Vector(po), wo) if po else kp_
        O[f'pole_knee_{sh}'].keyframe_insert('location', frame=fr)
        # feet on the pegs; only a shot's measured override takes one elsewhere (the recording has no foot channel)
        O[f'ik_foot_{sh}'].location = foot_at(sh, i)
        O[f'ik_foot_{sh}'].keyframe_insert('location', frame=fr)
    for b, m in bas.items():
        p = pb[b]
        loc, q, _ = m.decompose()
        p.rotation_quaternion = q
        p.keyframe_insert('rotation_quaternion', frame=fr)
        if b == 'riderRIG_Pelvis':
            p.location = loc
            p.keyframe_insert('location', frame=fr)
if USE_BONES:
    key_from_bones()
ad = arm.animation_data
if ad and ad.action:
    try:
        fcs = ad.action.fcurves
    except Exception:
        fcs = [fc for l in ad.action.layers for s in l.strips for cb in s.channelbags for fc in cb.fcurves]
    for fc in fcs:
        for k in fc.keyframe_points:
            k.interpolation = 'LINEAR'
# pole angles: mirrored bones want different ones, so pick per chain the angle that bends the mid joint toward its pole
sc.frame_set(40 if n >= 40 else 1)
for nm, sh, s in (() if USE_BONES else SIDES):
    for bn, root_b, pole in ((f'riderRIG_{nm}Elbow', f'riderRIG_{nm}Shoulder', f'pole_elbow_{sh}'), (f'riderRIG_{nm}Knee', f'riderRIG_{nm}Hip', f'pole_knee_{sh}')):
        c = next(c for c in pb[bn].constraints if c.type == 'IK')
        best = None
        for ang in range(-180, 180, 10):
            c.pole_angle = ang * DEG
            bpy.context.view_layer.update()
            mw = arm.matrix_world
            r0, mid, end = mw @ pb[root_b].head, mw @ pb[bn].head, mw @ pb[bn].tail
            axis = (end - r0).normalized()
            off = mid - r0
            off = off - axis * off.dot(axis)
            pv = O[pole].matrix_world.translation - r0
            pv = pv - axis * pv.dot(axis)
            sc_ = off.normalized().dot(pv.normalized()) if off.length > 1e-5 else -2
            if best is None or sc_ > best[0]:
                best = (sc_, ang)
        c.pole_angle = best[1] * DEG
        print('POSE pole', bn, best[1], round(best[0], 3))
# the wrist goes on the forearm's line: measure the solved forearm at a riding frame, put the wrist a hand short of
# the grip along it (steer frame), twice so the IK settles
fr_ref = 40 if n >= 40 else 1
for _ in range(0):                 # (was: wrist a hand short of the grip along the forearm; the wrist is placed by the grip now)
    sc.frame_set(fr_ref)
    bpy.context.view_layer.update()
    for nm, sh, s in SIDES:
        e = arm.matrix_world @ pb[f'riderRIG_{nm}Elbow'].head
        w = arm.matrix_world @ pb[f'riderRIG_{nm}Wrist'].head
        fdir = (ST_inv.to_3x3() @ (w - e)).normalized()
        g = grips[sh]
        S3 = (ST_inv @ ch.matrix_world).to_3x3()
        O[f'ik_hand_{sh}'].location = g - fdir * 0.075 + S3 @ Vector((0, 0, 0.02))


# fingers and thumb round the grip, solved against the bar at the riding frame: each finger's curl is scaled until
# its tip sits under/behind the rubber; the thumb is turned under the bar towards the stem (opposed, not hanging).
def tip_of(bn):
    return arm.matrix_world @ pb[bn].tail


sc.frame_set(fr_ref)
for nm, sh, s in SIDES:
    S3 = (ST_inv @ ch.matrix_world).to_3x3()
    St = steer.matrix_world
    gw = St @ grips[sh]
    want_f = St @ (grips[sh] + S3 @ Vector((0, 0.014, -0.019)))
    for fn in ('Index', 'Middle', 'Ring', 'Pink'):
        bs = [f'riderRIG_{nm}{fn}{j}' for j in (1, 2, 3) if f'riderRIG_{nm}{fn}{j}' in pb]
        if len(bs) < 3:
            continue
        base = [pb[b].rotation_quaternion.copy() for b in bs]
        best = None
        for kk in (0.5, 0.65, 0.8, 0.95, 1.1, 1.25, 1.4, 1.55):
            for b, q in zip(bs, base):
                pb[b].rotation_quaternion = Quaternion(q.axis, q.angle * kk) if q.angle > 1e-6 else q
            bpy.context.view_layer.update()
            tp = tip_of(bs[-1])
            dr = (tp - gw)
            ax = (St.to_3x3() @ (S3 @ Vector((1, 0, 0)))).normalized()
            radial = (dr - ax * dr.dot(ax)).length
            err = (tp - want_f).length + (0.05 if radial < 0.017 else 0.0)
            if best is None or err < best[0]:
                best = (err, kk)
        for b, q in zip(bs, base):
            pb[b].rotation_quaternion = Quaternion(q.axis, q.angle * best[1]) if q.angle > 1e-6 else q
        print('POSE finger', nm, fn, 'curl x', best[1], 'tip err', round(best[0], 3))
    tb = [f'riderRIG_{nm}Thumb{j}' for j in (1, 2, 3) if f'riderRIG_{nm}Thumb{j}' in pb]
    if len(tb) == 3:
        want_t = St @ (grips[sh] + S3 @ Vector((-s * 0.03, -0.008, -0.022)))
        best = None
        for axn in ((1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)):
            for ang in (15, 30, 45, 60, 75):
                pb[tb[0]].rotation_quaternion = Quaternion(axn, ang * DEG)
                for curl in (0, 25, 45):
                    for b in tb[1:]:
                        pb[b].rotation_quaternion = Quaternion((1, 0, 0), curl_sign(nm, b) * curl * DEG)
                    bpy.context.view_layer.update()
                    err = (tip_of(tb[-1]) - want_t).length
                    if best is None or err < best[0]:
                        best = (err, axn, ang, curl)
        pb[tb[0]].rotation_quaternion = Quaternion(best[1], best[2] * DEG)
        for b in tb[1:]:
            pb[b].rotation_quaternion = Quaternion((1, 0, 0), curl_sign(nm, b) * best[3] * DEG)
        print('POSE thumb', nm, 'axis', best[1], 'ang', best[2], 'curl', best[3], 'tip err', round(best[0], 3))
bpy.context.view_layer.update()

# The hand's position is the game's (gfx.cfg): the rider gfx.cfg lefthand/righthand `pos` is a point in the wrist
# bone's own frame and the bike gfx.cfg leftgrip/rightgrip is where the game puts it (steer frame). The turn is
# Sean's liked one (above); here the whole hand is slid so that point lands on the gfx grip, as the game's IK does.
GFX_GRIP = {}
for nm, sh, s in (() if USE_BONES else SIDES):        # recorded bones: the game already put the hand there
    hb = _cfg_block(RIDER_GFX, ('lefthand', 'righthand')[sh == 'R'])
    wpos = _cfg_pos(hb) if hb else Vector((0.08, -0.02 * s, 0.02))
    gb = _cfg_block(_steer_blk, ('leftgrip', 'rightgrip')[sh == 'R'])
    if not gb:
        continue
    grip_bl = M3G @ _cfg_pos(gb)                            # steer-local, Blender
    GFX_GRIP[sh] = grip_bl
    wb = f'riderRIG_{nm}Wrist'
    if wb not in RBONES or wb not in pb:
        continue
    rest_pt = M3G @ (RBONES[wb] @ wpos)                     # the gfx point on the rest body (Blender model space)
    for _ in range(3):
        sc.frame_set(fr_ref)
        bpy.context.view_layer.update()
        Mw = arm.matrix_world @ pb[wb].matrix @ pb[wb].bone.matrix_local.inverted()
        now = steer.matrix_world.inverted() @ (Mw @ rest_pt)
        d_ = grip_bl - now
        for e_ in (f'ik_hand_{sh}', f'cin_grip_{sh}', f'cin_bar_{sh}'):
            O[e_].location = O[e_].location + d_
    print('POSE hand', sh, 'on the gfx grip', tuple(round(x, 3) for x in grip_bl), 'last correction mm', round(d_.length * 1000, 1))
if os.environ.get('CIN_MARKERS'):
    mm = bpy.data.materials.new('cin_marker')
    mm.use_nodes = True
    em = mm.node_tree.nodes.new('ShaderNodeEmission')
    em.inputs['Color'].default_value = (1, 0.05, 0.6, 1)
    em.inputs['Strength'].default_value = 8
    mm.node_tree.links.new(em.outputs[0], mm.node_tree.nodes['Material Output'].inputs['Surface'])
    for tag, par, loc in [(f'gfxgrip_{k}', steer, v) for k, v in GFX_GRIP.items()] + [(f'peg_{k}', ch, v) for k, v in pegs.items()]:
        bpy.ops.mesh.primitive_uv_sphere_add(radius=0.012, segments=12, ring_count=8)
        mk = bpy.context.object
        mk.name = 'MARK_' + tag
        for c_ in mk.users_collection:
            c_.objects.unlink(mk)
        bpy.data.collections['rider'].objects.link(mk)
        mk.data.materials.append(mm)
        mk.parent = par
        mk.matrix_parent_inverse = Matrix.Identity(4)
        mk.location = loc


# The glove must not go into the grip, bar or levers. Measured against the real meshes (BVH nearest point + normal:
# a glove vertex behind a control's surface is inside it, by that distance). Per hand: lift the whole hand off the
# bar along its palm normal (wrist target and both aim targets together, so the turn Sean liked is unchanged) to the
# smallest lift with the palm/back clear, +1 mm; then open each finger only as far as it takes to come out.
from mathutils.bvhtree import BVHTree
body_o = O.get('rider_body')
dom_ = np.array([max(v.groups, key=lambda g: g.weight).group if len(v.groups) else -1 for v in body_o.data.vertices])
gidx_ = {g.name: g.index for g in body_o.vertex_groups}
CTRL_KEYS = ('grip', 'renthal', 'fatbar', 'twinwall')   # closed tubes: inside = behind the surface (thin shells like handguards can't be tested this way)
ctrl = [o for o in bike_meshes if under(o, 'steer') and (any(k in o.name.lower() for k in CTRL_KEYS) or
                                                         any(m and 'grip' in m.name.lower() for m in o.data.materials))]
sc.frame_set(fr_ref)
STi = steer.matrix_world.inverted()
CTRL_BVH = []
for o in ctrl:                                            # one tree per part: a rubber's own bore counts as its surface
    V = verts_in(o, STi)
    if any(k in o.name.lower() for k in CTRL_KEYS):
        polys = [tuple(p_.vertices) for p_ in o.data.polygons]
    else:
        _, polys = part_faces(o, ('grip',))               # only the rubber out of a merged steer mesh
    if polys:
        CTRL_BVH.append(BVHTree.FromPolygons([Vector(v) for v in V], polys))


def hand_verts_steer():
    bpy.context.view_layer.update()
    dg_ = bpy.context.evaluated_depsgraph_get()
    ev_ = body_o.evaluated_get(dg_)
    me_ = ev_.to_mesh()
    co = np.empty(len(me_.vertices) * 3, np.float32)
    me_.vertices.foreach_get('co', co)
    Wl = np.array(steer.matrix_world.inverted() @ ev_.matrix_world)
    ev_.to_mesh_clear()
    return co.reshape(-1, 3) @ Wl[:3, :3].T + Wl[:3, 3]


RAYS = (Vector((0.0, 0.6, 0.8)), Vector((0.0, -0.8, 0.6)), Vector((0.57, 0.57, -0.59)))


def inside(t, vv):
    """ray parity against one part (robust to open/odd normals): inside if most of 3 rays cross it an odd number of times"""
    odd = 0
    for d in RAYS:
        d = d.normalized()
        org, n_ = vv.copy(), 0
        for _ in range(20):
            loc, nrm_, idx_, dist = t.ray_cast(org, d)
            if loc is None:
                break
            n_ += 1
            org = loc + d * 1e-5
        odd += n_ % 2
    return odd >= 2


def depth_of(Pv, ids):
    worst = 0.0
    for v in Pv[ids]:
        vv = Vector(v)
        for t in CTRL_BVH:
            hit = t.find_nearest(vv, 0.03)
            if hit[0] is None:
                continue
            dl = (vv - hit[0]).length
            if dl > worst and inside(t, vv):
                worst = dl
    return worst


# twist bones: the game drives them, Blender doesn't. The IK rolls the upper arm ~75 deg to put the elbows out and the
# hand's roll onto the bar twists the wrist; on the main bones alone that wrings the skin at the shoulder (the tear)
# and at the wrist. Bake per frame: the shoulder skin takes back most of the humerus roll, the forearm ramps the
# wrist's roll from elbow to wrist, the thigh root takes back half the hip roll.
def twist_y(child, parent):
    rest = REST[child].to_3x3()
    prest = REST[parent].to_3x3()
    q = ((prest.inverted() @ rest).inverted() @ (pb[parent].matrix.to_3x3().inverted() @ pb[child].matrix.to_3x3())).to_quaternion()
    t = 2 * math.atan2(q.y, q.w)
    return (t + math.pi) % (2 * math.pi) - math.pi


TW = []
for nm, sh, s in SIDES:
    TW += [(f'riderRIG_{nm}ShoulderTwist1', f'riderRIG_{nm}Shoulder', f'riderRIG_{nm}Collar', -0.7),
           (f'riderRIG_{nm}ShoulderTwist2', f'riderRIG_{nm}Shoulder', f'riderRIG_{nm}Collar', -0.35),
           (f'riderRIG_{nm}ElbowTwist1', f'riderRIG_{nm}Wrist', f'riderRIG_{nm}Elbow', 0.33),
           (f'riderRIG_{nm}ElbowTwist2', f'riderRIG_{nm}Wrist', f'riderRIG_{nm}Elbow', 0.66),
           (f'riderRIG_{nm}HipTwist1', f'riderRIG_{nm}Hip', 'riderRIG_Pelvis', -0.5)]
TW = [t for t in TW if all(b in pb for b in t[:3])]
if USE_BONES:                                        # the game's own twist bones are in the recording
    TW = [t for t in TW if t[0] not in F[_bf[0]]['bones']]
for own, *_ in TW:
    pb[own].rotation_mode = 'QUATERNION'
mx = {}
# Boot flex: the game's boot is rigid on the shin twist bone (no ankle). Cinema lets the foot turn towards flat on
# the peg (the cin_footref empty) by at most BOOT_FLEX deg off the shin, the flex a real boot has; on a shot's
# measured pose (override frames) none, so the boot sits on the shin exactly as the game draws it.
BOOT_FLEX = 15.0
A3i = arm.matrix_world.to_3x3().normalized().inverted()
foot_dev = {'L': 0.0, 'R': 0.0}


def bake_foot(fr):
    i = fr - 1
    cap = 0.0 if USE_BONES else BOOT_FLEX * DEG * (1.0 - ovr_w(i))   # recorded pose: rigid on KneeTwist, as drawn
    for nm, sh, s in SIDES:
        fb_, kt_ = f'cin_foot_{sh}', f'riderRIG_{nm}KneeTwist'
        if fb_ not in pb or kt_ not in pb:
            continue
        Rf = REST[fb_].to_3x3()
        follow = (pb[kt_].matrix.to_3x3() @ REST[kt_].to_3x3().inverted()) @ Rf
        flat = A3i @ O[f'cin_footref_{sh}'].matrix_world.to_3x3().normalized()
        dq = (flat @ follow.inverted()).to_quaternion()
        foot_dev[sh] = max(foot_dev[sh], math.degrees(dq.angle))
        if dq.angle > cap:
            dq = Quaternion(dq.axis, cap) if cap > 1e-6 else Quaternion()
        M = dq.to_matrix() @ follow
        kn = pb[f'riderRIG_{nm}Knee']
        basis = ((REST[kn.name].to_3x3().inverted() @ Rf).inverted() @ kn.matrix.to_3x3().inverted() @ M)
        pb[fb_].rotation_quaternion = basis.to_quaternion()
        pb[fb_].keyframe_insert('rotation_quaternion', frame=fr)


for fr in range(1, n + 1):
    sc.frame_set(fr)
    bake_foot(fr)
    tws = {}
    for own, src, par, k in TW:
        key = (src, par)
        if key not in tws:
            tws[key] = twist_y(src, par)
        ang = k * tws[key]
        mx[own] = max(mx.get(own, 0.0), abs(math.degrees(tws[key])))
        pb[own].rotation_quaternion = Quaternion((0, 1, 0), ang)
        pb[own].keyframe_insert('rotation_quaternion', frame=fr)
print('POSE twist baked, max source twist', {k.replace('riderRIG_', ''): round(v) for k, v in mx.items()})
print('POSE boot flex: largest foot-flat vs shin turn wanted', {k: round(v, 1) for k, v in foot_dev.items()}, 'deg, capped at', BOOT_FLEX)


HAND_DEPTH = {}
# The liked hand stays exactly where it is (no lift, no re-turn). The game's glove is shaped for a thinner grip than
# this bike's 35 mm ODI rubber, so where the glove skin is inside the rubber/bar it is pushed out to the surface,
# 1.2 mm proud, as a corrective shape key (rest space, through each vertex's bone), measured with a ray-parity
# inside test against the real grip/bar meshes. Iterated until 0 mm; then checked on every frame.
if os.environ.get('CIN_HANDFIX', '1') != '0' and CTRL_BVH:   # no grip in the bike: nothing to keep the glove out of
    hand_ids = {}
    for nm, sh, s in SIDES:
        keys = [f'riderRIG_{nm}Wrist'] + [f'riderRIG_{nm}{fn}{j}' for fn in ('Index', 'Middle', 'Ring', 'Pink', 'Thumb') for j in (1, 2, 3)]
        hand_ids[sh] = np.nonzero(np.isin(dom_, [gidx_[k] for k in keys if k in gidx_]))[0]
    allh = np.concatenate(list(hand_ids.values()))
    _ks = body_o.data.shape_keys
    if _ks and 'cin_glove_clear' in _ks.key_blocks and 'Basis' in _ks.key_blocks:   # a re-run on a saved scene:
        _kb = _ks.key_blocks['Basis'].data                                          # start from the bare glove, not
        for _v, _d in zip(_kb, _ks.key_blocks['cin_glove_clear'].data):             # the last run's push
            _d.co = _v.co
    sc.frame_set(fr_ref)
    before = {sh: depth_of(hand_verts_steer(), ids) for sh, ids in hand_ids.items()}
    # 1) along the bar: the fist sits on the rubber, off the throttle housing / lock-on flange (where the grip mesh is
    #    wider than ~22 mm); slide the whole hand out along the bar until its inner edge clears that collar by 2 mm
    for nm, sh, s in SIDES:
        STi = steer.matrix_world.inverted()
        gw = np.concatenate([verts_of(o, STi, ('grip',)) for o in ctrl] + [np.zeros((0, 3))])
        gw = gw[gw[:, 0] * s > 0.15]
        if len(gw) < 10:
            continue
        ax_ = np.abs(gw[:, 0])
        collar_end = None
        for x0 in np.arange(ax_.min(), ax_.max(), 0.002):
            sel = np.abs(ax_ - x0) < 0.002
            if sel.sum() < 10:
                continue
            Y, Z = gw[sel, 1], gw[sel, 2]
            sol = np.linalg.lstsq(np.c_[2 * Y, 2 * Z, np.ones(len(Y))], Y ** 2 + Z ** 2, rcond=None)[0]
            if np.percentile(np.hypot(Y - sol[0], Z - sol[1]), 95) > 0.022:
                collar_end = x0
        Pv = hand_verts_steer()
        ids = hand_ids[sh]
        fing = np.nonzero(np.isin(dom_, [gidx_[f'riderRIG_{nm}{fn}{j}'] for fn in ('Index', 'Middle', 'Ring', 'Pink') for j in (1, 2, 3)
                                          if f'riderRIG_{nm}{fn}{j}' in gidx_]))[0]
        inner = float(np.abs(Pv[fing, 0]).min())          # the fist's inner side: the index finger, not the cuff
        if collar_end is not None and inner < collar_end + 0.002:
            d_ = collar_end + 0.002 - inner
            for e_ in (f'ik_hand_{sh}', f'cin_grip_{sh}', f'cin_bar_{sh}'):
                O[e_].location = O[e_].location + Vector((s * d_, 0, 0))
        print('POSE hand', sh, 'collar ends at', None if collar_end is None else round(collar_end, 3), 'hand inner edge', round(inner, 3))

    me0 = body_o.data
    if not me0.shape_keys:
        body_o.shape_key_add(name='Basis', from_mix=False)
    sk = me0.shape_keys.key_blocks.get('cin_glove_clear') or body_o.shape_key_add(name='cin_glove_clear', from_mix=False)
    sk.value = 1.0
    hv = body_o.vertex_groups.get('cin_hands') or body_o.vertex_groups.new(name='cin_hands')
    hv.add([int(i) for i in allh], 1.0, 'REPLACE')
    for m_ in body_o.modifiers:
        if m_.type == 'CORRECTIVE_SMOOTH':
            m_.vertex_group = 'cin_hands'
            m_.invert_vertex_group = True
    bone_of = {int(v): body_o.vertex_groups[int(dom_[v])].name for v in allh}
    Aw = arm.matrix_world
    basis_co = np.array([kb.co[:] for kb in me0.shape_keys.key_blocks['Basis'].data])
    hset = set(int(v) for v in allh)
    nbr = {}
    for e_ in me0.edges:
        a_, b_ = e_.vertices
        if a_ in hset and b_ in hset:
            nbr.setdefault(a_, set()).add(b_)
            nbr.setdefault(b_, set()).add(a_)
    keyp = {}
    for v_ in hset:
        keyp.setdefault(tuple(np.round(basis_co[v_], 5)), []).append(v_)
    for grp_ in keyp.values():                            # UV-seam twins share their neighbourhoods
        un = set().union(*[nbr.get(u, set()) for u in grp_])
        for u in grp_:
            nbr[u] = un - {u}
    hl = sorted(hset)
    D = {v: Vector((0, 0, 0)) for v in hl}                 # rest-space push per glove vertex

    # The push is skin, not shape: at most GLOVE_MAX per vertex. It is baked once in rest space and applies on every
    # frame, so a frame where the arm can't reach the grip used to inflate the fingers by up to 41 mm on all frames
    # (the stretched, swollen glove). Deeper overlap than this is left to the hand pose, not the glove's shape.
    GLOVE_MAX = 0.004

    def write_D():
        for v in hl:
            if D[v].length > GLOVE_MAX:
                D[v] = D[v].normalized() * GLOVE_MAX
        for grp_ in keyp.values():
            dm = max((D[u] for u in grp_), key=lambda d: d.length)
            for u in grp_:
                D[u] = dm.copy()
        for v in hl:
            sk.data[v].co = Vector(basis_co[v]) + D[v]

    def requirement(frames):
        """per glove vertex, the rest-space push that takes it to the rubber/bar surface + 1.5 mm (worst over frames)"""
        req = {}
        for fr_ in frames:
            sc.frame_set(fr_)
            Pv = hand_verts_steer()
            St3 = steer.matrix_world.to_3x3()
            for v in hl:
                vv = Vector(Pv[v])
                best = None
                for t in CTRL_BVH:
                    hit = t.find_nearest(vv, 0.04)
                    if hit[0] is None or not inside(t, vv):
                        continue
                    if best is None or (vv - hit[0]).length > (vv - best).length:
                        best = hit[0]
                if best is None:
                    continue
                tgt = best + (best - vv).normalized() * 0.0025
                pbn = pb.get(bone_of[v])
                Rb = (pbn.matrix @ pbn.bone.matrix_local.inverted()).to_3x3() if pbn else Matrix.Identity(3)
                d_rest = Rb.inverted() @ (Aw.to_3x3().inverted() @ (St3 @ (tgt - vv)))
                if v not in req or d_rest.length > req[v].length:
                    req[v] = d_rest
        return req

    frames_all = list(range(1, n + 1, 6))
    for rnd in range(9):
        req = requirement([fr_ref] if rnd < 3 else frames_all)
        if not req:
            break
        for v, d in req.items():                          # add exactly what is missing
            D[v] = D[v] + d
        for _ in range(2):                                # smooth the field so it is a soft bulge, never a spike
            D2 = {}
            for v in hl:
                ns = nbr.get(v)
                D2[v] = D[v] * 0.5 + (sum((D[u] for u in ns), Vector((0, 0, 0))) / len(ns)) * 0.5 if ns else D[v]
            D = D2
        write_D()
        print('POSE glove round', rnd, 'vertices short', len(req))
    # final: anything still in on any frame gets exactly its push, unsmoothed but tiny
    for rnd in range(16):
        req = requirement(frames_all if rnd < 2 else list(range(1, n + 1)))
        req = {v: d for v, d in req.items() if D[v].length < GLOVE_MAX - 1e-6}   # capped ones stay where they are
        if not req:
            break
        for v, d in req.items():
            D[v] = D[v] + d
        write_D()
        print('POSE glove final round', rnd, 'vertices short', len(req))
    Dm_all = np.array([D[v].length for v in hl])
    top = np.argsort(-Dm_all)[:8]
    print('POSE glove push largest mm', [(bone_of[hl[i]].replace('riderRIG_', ''), round(float(Dm_all[i]) * 1000, 1)) for i in top],
          'verts pushed', int((Dm_all > 1e-4).sum()), 'mean mm', round(float(Dm_all[Dm_all > 1e-4].mean()) * 1000, 1) if (Dm_all > 1e-4).any() else 0)
    sc.frame_set(fr_ref)
    after = {sh: depth_of(hand_verts_steer(), ids) for sh, ids in hand_ids.items()}
    for sh in hand_ids:
        HAND_DEPTH[sh] = (before[sh], after[sh])
        print(f'POSE hand {sh}: glove penetration into grip/bar before {before[sh] * 1000:.1f} mm -> after {after[sh] * 1000:.1f} mm (hand not moved; glove skin pushed out)')



sc.frame_set(1)
for k in (() if USE_BONES else (40, 128, 181)):
    i = k - 1
    if i < n:
        print(f'POSE f{k} stand {stand[i]:.2f} fb {fbs[i]:+.2f} lr {lrs[i]:+.2f} roll {roll[i]:+.1f} pitch {pitch[i]:+.1f} crouch {crouch[i]:.3f}')
print('POSE done frames', n, 'from recorded bones' if USE_BONES else '', 'bar keep-out frames', sum(1 for k_, _ in stats if k_ < 1), 'seat lifts', sum(1 for _, d_ in stats if d_ > 0), 'max lift', round(max([d_ for _, d_ in stats] + [0.0]), 3))

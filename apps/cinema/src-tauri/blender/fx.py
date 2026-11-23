"""MXB Cinema effects: roost, dust trail, landing puff, tyre marks. Runs after drive_scene.py, before the render script:
  blender -b scene.blend -P animate.py -P drive_scene.py -P ../fx/fx.py -P shots.py -- --json clip.json ...
Everything is driven by the replay telemetry (wheel speed vs ground speed, airborne flag, throttle) plus a
ground-contact test of each axle against track_terrain_detail.
  * particles are precomputed here (seeded, deterministic) and stored as point attributes; a geometry-nodes
    tree evaluates them analytically at the scene frame (ballistic with linear drag, settled on ground hit),
    so any frame renders on its own, no simulation cache, and motion blur gets the right subframe position.
  * roost: clods (instanced rocks) + fine grit (point cloud) from the rear contact patch, scaled by slip/throttle.
  * dust: soft volume puffs (Principled Volume in instanced spheres) that grow, drift with the wind and fade.
  * landing: burst of dust, clods and grit at each wheel's touchdown, scaled by the axle's sink speed.
  * tyre marks: a 1 cm/px mask (depth with tread, side lip, birth frame) baked from the contact path, read by
    track_dirt_displaced: darker, smoother, bump-depressed tracks that appear only once the wheel has passed.
Flags: --fx-seed N, --fx-off roost,dust,landing,marks, --fx-scale F (particle count multiplier)."""
import bpy, sys, os, math, json, argparse, time
import numpy as np
from mathutils import Vector, Matrix

_t0 = time.time()
argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
ap = argparse.ArgumentParser()
ap.add_argument('--json', required=True)
ap.add_argument('--fx-seed', type=int, default=7)
ap.add_argument('--fx-off', default='')
ap.add_argument('--fx-scale', type=float, default=1.0)
a, _ = ap.parse_known_args(argv)
OFF = set(x for x in a.fx_off.split(',') if x)

sc = bpy.context.scene
O = bpy.data.objects
root = O['CINEMA_bike_root']
D = json.load(open(a.json))
FR = D['riders'][0]['frames']
FPS = float(D['fps'])
RAD_F, RAD_R = D.get('wheel_radius', [0.36, 0.35])
n = len(FR)
rng = np.random.default_rng(a.fx_seed)
GRAV = np.array([0, 0, -9.81])


def coll(name):
    c = bpy.data.collections.get(name) or bpy.data.collections.new(name)
    if c.name not in sc.collection.children:
        sc.collection.children.link(c)
    return c


FXC = coll('FX')
for o in list(FXC.objects):
    bpy.data.objects.remove(o, do_unlink=True)

# ---- ground height grid from the detail patch (a regular heightmap grid in Blender XY) -------------------
td = O['track_terrain_detail']
co = np.empty(len(td.data.vertices) * 3, np.float32)
td.data.vertices.foreach_get('co', co)
co = co.reshape(-1, 3) @ np.array(td.matrix_world.to_3x3()).T + np.array(td.matrix_world.translation)
ux = np.unique(np.round(co[:, 0], 4))
mps = float(np.median(np.diff(ux)))
gx0, gy0 = co[:, 0].min(), co[:, 1].min()
ci = np.round((co[:, 0] - gx0) / mps).astype(int)
ri = np.round((co[:, 1] - gy0) / mps).astype(int)
HG = np.full((ri.max() + 1, ci.max() + 1), np.nan, np.float32)
HG[ri, ci] = co[:, 2]
HG = np.where(np.isnan(HG), np.nanmin(HG) - 50, HG)
del co


def ground(x, y):
    """Bilinear ground height (no fine displacement); far below outside the patch."""
    fx = np.clip((np.asarray(x) - gx0) / mps, 0, HG.shape[1] - 1.001)
    fy = np.clip((np.asarray(y) - gy0) / mps, 0, HG.shape[0] - 1.001)
    i0, j0 = fy.astype(int), fx.astype(int)
    u, v = fx - j0, fy - i0
    return (HG[i0, j0] * (1 - u) * (1 - v) + HG[i0, j0 + 1] * u * (1 - v) +
            HG[i0 + 1, j0] * (1 - u) * v + HG[i0 + 1, j0 + 1] * u * v)


def gnormal(x, y, e=0.15):
    dzx = (ground(x + e, y) - ground(x - e, y)) / (2 * e)
    dzy = (ground(x, y + e) - ground(x, y - e)) / (2 * e)
    nv = np.stack([-dzx, -dzy, np.ones_like(dzx)], -1)
    return nv / np.linalg.norm(nv, axis=-1, keepdims=True)


# ---- per-frame bike state (index i = frame i+1) --------------------------------------------------------------
AX = {'f': [], 'r': []}
FWD, UP, POS = [], [], []
for fr in range(1, n + 1):
    sc.frame_set(fr)
    AX['f'].append(np.array(O['fwheel'].matrix_world.translation))
    AX['r'].append(np.array(O['rwheel'].matrix_world.translation))
    R = root.matrix_world.to_3x3()
    FWD.append(np.array(R @ Vector((0, -1, 0))))
    UP.append(np.array(R @ Vector((0, 0, 1))))
    POS.append(np.array(root.matrix_world.translation))
sc.frame_set(1)
AX = {k: np.array(v) for k, v in AX.items()}
FWD, UP, POS = np.array(FWD), np.array(UP), np.array(POS)
VEL = np.gradient(POS, axis=0) * FPS
spd = np.linalg.norm(VEL[:, :2], axis=1)
wr = np.array([f['wheel_rot'] for f in FR])
wspd_r = np.gradient(wr[:, 1]) * FPS * RAD_R
thr = np.array([float(f.get('throttle') or 0) for f in FR])
air = np.array([bool(f.get('airborne')) for f in FR])
slip = wspd_r - spd
RADS = {'f': RAD_F, 'r': RAD_R}


def contact(w, i):
    """(touching, contact point) of wheel w at frame index i, from the axle height above the ground grid."""
    ax, up = AX[w][i], UP[i]
    cp = ax - up * RADS[w]
    g = ground(cp[0], cp[1])
    return (ax[2] - ground(ax[0], ax[1])) < RADS[w] + 0.07, np.array([cp[0], cp[1], g])


CON = {w: np.array([contact(w, i)[0] for i in range(n)]) for w in 'fr'}
CPT = {w: np.array([contact(w, i)[1] for i in range(n)]) for w in 'fr'}
print('FX contact frames f', int(CON['f'].sum()), 'r', int(CON['r'].sum()), 'airborne flag', int(air.sum()), 'mps', round(mps, 3))

# ---- track dirt colour (the base sheet as graded by the terrain material) ---------------------------------
im = bpy.data.images.get('ground_surface0.png') or bpy.data.images.get('ground_surface0')
if im:
    px = np.empty(im.size[0] * im.size[1] * 4, np.float32)
    im.pixels.foreach_get(px)
    DIRT = px.reshape(-1, 4)[:, :3].mean(0) * 0.72
else:
    DIRT = np.array([0.27, 0.24, 0.21])
print('FX dirt colour', np.round(DIRT, 3))

# ---- particle generation ---------------------------------------------------------------------------------------
P = {k: {c: [] for c in ('p0', 'v0', 'fb', 'k', 'vt', 'r0', 'd0', 'life', 'rot', 'spin', 'var')} for k in ('clod', 'grit', 'dust')}
WIND = np.array([0.0, 0.0, 0.0])
_h = np.array([FWD[:, 0].mean(), FWD[:, 1].mean(), 0.0])
_h /= np.linalg.norm(_h)
WIND = np.array([-_h[1], _h[0], 0.0]) * 0.7 + _h * 0.15      # light cross-wind, a touch with the riding direction
DUST_RISE = np.array([0, 0, 0.12])


def emit(kind, N, fb, p0, v0, k, r0, d0=0.0, life=1e3, spin=0.0):
    if N <= 0:
        return
    d = P[kind]
    d['p0'].append(p0)
    d['v0'].append(v0)
    d['fb'].append(np.broadcast_to(fb, (N,)).astype(np.float32))
    d['k'].append(np.broadcast_to(k, (N,)).astype(np.float32))
    vt = (GRAV / np.maximum(np.asarray(k), 1e-3)[..., None] if kind != 'dust' else np.broadcast_to(WIND + DUST_RISE, (N, 3))) + (WIND if kind != 'dust' else 0)
    d['vt'].append(np.broadcast_to(vt, (N, 3)).astype(np.float32))
    d['r0'].append(np.asarray(r0, np.float32))
    d['d0'].append(np.broadcast_to(d0, (N,)).astype(np.float32))
    d['life'].append(np.broadcast_to(life, (N,)).astype(np.float32))
    d['rot'].append(rng.uniform(0, 2 * math.pi, (N, 3)).astype(np.float32))
    d['spin'].append((rng.normal(0, 1, (N, 3)) * spin).astype(np.float32))
    d['var'].append(rng.integers(0, 6, N).astype(np.int32))


def spray_dirs(N, back, side, up, elev, yaw):
    """Unit vectors around 'back', elevation elev (rad) towards 'up', yaw (rad) towards 'side'."""
    ce, se = np.cos(elev)[:, None], np.sin(elev)[:, None]
    cy, sy = np.cos(yaw)[:, None], np.sin(yaw)[:, None]
    return ce * (cy * back + sy * side) + se * up


def frame_basis(i, cpt):
    nrm = gnormal(cpt[0], cpt[1])
    f = FWD[i] - nrm * FWD[i].dot(nrm)
    f /= np.linalg.norm(f) + 1e-9
    s = np.cross(nrm, f)
    return f, s, nrm


S = a.fx_scale
# 1) roost + tyre dust from the rear contact patch
for i in range(n - 1):
    if not CON['r'][i]:
        continue
    sl = max(slip[i], 0.0)
    I = min(1.6, 0.35 * thr[i] + 0.3 * sl)          # roost intensity
    f, s, nrm = frame_basis(i, CPT['r'][i])
    for kind, rate in (('clod', 4.0), ('grit', 70.0)):
        if 'roost' in OFF:
            break
        N = rng.poisson(rate * I * S)
        if N == 0:
            continue
        u = rng.uniform(0, 1, N)
        cp = CPT['r'][i] * (1 - u[:, None]) + CPT['r'][i + 1] * u[:, None]
        p0 = cp - f * rng.uniform(0.05, 0.2, (N, 1)) + s * rng.normal(0, 0.035, (N, 1)) + nrm * rng.uniform(0.01, 0.06, (N, 1))
        elev = np.radians(rng.uniform(8, 38, N) if kind == 'clod' else rng.uniform(4, 50, N))
        yaw = np.radians(rng.normal(0, 9 if kind == 'clod' else 14, N))
        dirs = spray_dirs(N, -f, s, nrm, elev, yaw)
        sp = (2.5 + 1.6 * sl + 3.5 * thr[i]) * rng.uniform(0.55, 1.15, N)
        sp = np.minimum(sp, 17.0)
        v0 = dirs * sp[:, None] + VEL[i] * 0.12
        if kind == 'clod':
            r0 = np.exp(rng.normal(np.log(0.014), 0.45, N))
            emit('clod', N, i + 1 + u, p0, v0, 0.12, np.minimum(r0, 0.05), spin=9.0)
        else:
            r0 = rng.uniform(0.0018, 0.0055, N)
            emit('grit', N, i + 1 + u, p0, v0, 0.9, r0)
    if 'dust' in OFF:
        continue
    # dust: always some off a rolling tyre on dry dirt, more with slip and throttle
    Id = 0.45 + 0.35 * thr[i] + 0.25 * min(sl, 3.0)
    N = rng.poisson(1.3 * Id * S)
    if N:
        u = rng.uniform(0, 1, N)
        cp = CPT['r'][i] * (1 - u[:, None]) + CPT['r'][i + 1] * u[:, None]
        p0 = cp - f * rng.uniform(0.3, 0.7, (N, 1)) + s * rng.normal(0, 0.08, (N, 1)) + nrm * rng.uniform(0.15, 0.3, (N, 1))
        v0 = spray_dirs(N, -f, s, nrm, np.radians(rng.uniform(10, 30, N)), np.radians(rng.normal(0, 15, N))) * rng.uniform(2.0, 4.5, (N, 1))
        emit('dust', N, i + 1 + u, p0, v0, 1.8, rng.uniform(0.22, 0.38, N), d0=rng.uniform(1.6, 2.6, N) * Id, life=1e3)
    # front tyre: light dust only
    if CON['f'][i]:
        N = rng.poisson(0.25 * S)
        if N:
            ff, ss, nn = frame_basis(i, CPT['f'][i])
            p0 = CPT['f'][i] + nn * 0.15 + ss * rng.normal(0, 0.05, (N, 1)) - ff * 0.2
            v0 = (-ff * rng.uniform(0.5, 2.0, (N, 1)) + nn * 0.4)
            emit('dust', N, i + 1 + rng.uniform(0, 1, N), p0, v0, 1.8, rng.uniform(0.18, 0.28, N), d0=1.2, life=1e3)

# 2) landing: each wheel's first contact after >= 8 frames in the air, scaled by the axle's sink speed
landings = []
for w in 'fr':
    run = 0
    for i in range(n):
        if not CON[w][i]:
            run += 1
            continue
        vz_ = (AX[w][i, 2] - AX[w][max(0, i - 2), 2]) * FPS / 2
        if run >= 8 and vz_ < -1.5:
            vz = (AX[w][i, 2] - AX[w][max(0, i - 2), 2]) * FPS / 2
            landings.append((w, i, vz))
        run = 0
for w, i, vz in landings:
    st = float(np.clip(abs(vz) / 4.0, 0.3, 1.6)) * (1.0 if w == 'r' else 0.7)
    cp = CPT[w][i]
    f, s, nrm = frame_basis(i, cp)
    sp_bike = spd[i]
    print('FX landing', w, 'frame', i + 1, 'vz', round(vz, 2), 'strength', round(st, 2))
    if 'landing' in OFF:
        continue
    for kind, num in (('dust', 22), ('clod', 45), ('grit', 450)):
        N = int(num * st * S)
        ang = rng.uniform(0, 2 * math.pi, N)
        # mostly sideways and back: the bike carries on forward over its own splash
        radial = np.cos(ang)[:, None] * s + np.sin(ang)[:, None] * f
        radial = np.where((radial @ f)[:, None] > 0.2, radial * np.array([1, 1, 1]) - f * 0.6, radial)
        radial /= np.linalg.norm(radial, axis=1, keepdims=True)
        u = rng.uniform(0, 0.6, N)
        p0 = cp + radial * rng.uniform(0.05, 0.3, (N, 1)) * (1 if kind != 'dust' else 2.5) + nrm * (0.03 if kind != 'dust' else 0.15)
        if kind == 'dust':
            el = np.radians(rng.uniform(5, 25, N))
            v0 = (np.cos(el)[:, None] * radial + np.sin(el)[:, None] * nrm) * (rng.uniform(2.5, 6.5, (N, 1)) * st) + VEL[i] * 0.25
            emit('dust', N, i + 1 + u * 0.5, p0, v0, 2.2, rng.uniform(0.25, 0.5, N), d0=rng.uniform(1.3, 2.2, N) * min(st, 1.0), life=1e3)
        else:
            el = np.radians(rng.uniform(15, 60, N))
            v0 = (np.cos(el)[:, None] * radial + np.sin(el)[:, None] * nrm) * (rng.uniform(1.5, 5.5, (N, 1)) * st) + VEL[i] * 0.35
            if kind == 'clod':
                emit('clod', N, i + 1 + u * 0.3, p0, v0, 0.12, np.minimum(np.exp(rng.normal(np.log(0.013), 0.5, N)), 0.05), spin=8.0)
            else:
                emit('grit', N, i + 1 + u * 0.3, p0, v0, 0.9, rng.uniform(0.0018, 0.005, N))


# ---- ground collision (clods settle where they land, grit disappears into the dirt) -----------------------
def finalize(kind):
    d = P[kind]
    if not d['p0']:
        return None
    A = {c: np.concatenate(v) for c, v in d.items()}
    N = len(A['fb'])
    if kind in ('clod', 'grit'):
        tmax = (n - A['fb']) / FPS + 0.1
        ts = np.arange(0.004, 3.5, 0.004)
        k = A['k'][:, None, None]
        e = (1 - np.exp(-k * ts[None, :, None])) / k
        hit = np.full(N, 1e3, np.float32)
        for c0 in range(0, N, 1500):
            sl_ = slice(c0, c0 + 1500)
            pos = A['p0'][sl_, None] + A['vt'][sl_, None] * ts[None, :, None] + (A['v0'][sl_] - A['vt'][sl_])[:, None] * e[sl_]
            below = pos[..., 2] < ground(pos[..., 0], pos[..., 1]) + 0.0
            # ignore the first few ms (emitted right at the surface)
            below[:, :4] = False
            fi = np.where(below.any(1), below.argmax(1), -1)
            hit[sl_] = np.where(fi >= 0, ts[np.maximum(fi, 0)], 1e3)
        if kind == 'clod':
            A['life'] = np.full(N, 1e3, np.float32)
            A['th'] = hit
        else:
            A['life'] = hit
            A['th'] = np.full(N, 1e3, np.float32)
        print('FX', kind, N, 'grounded', int((hit < 1e3).sum()))
    else:
        A['th'] = np.full(N, 1e3, np.float32)
        print('FX dust', N)
    return A


# ---- geometry-nodes evaluator ------------------------------------------------------------------------------
class G:
    def __init__(self, name):
        old = bpy.data.node_groups.get(name)
        if old:
            bpy.data.node_groups.remove(old)
        self.t = bpy.data.node_groups.new(name, 'GeometryNodeTree')
        self.t.interface.new_socket('Geometry', in_out='INPUT', socket_type='NodeSocketGeometry')
        self.t.interface.new_socket('Geometry', in_out='OUTPUT', socket_type='NodeSocketGeometry')
        self.N, self.L = self.t.nodes, self.t.links
        self.gin = self.N.new('NodeGroupInput')
        self.gout = self.N.new('NodeGroupOutput')

    def node(self, typ, **props):
        nd = self.N.new(typ)
        for k, v in props.items():
            setattr(nd, k, v)
        return nd

    def link(self, sock, dst):
        if isinstance(sock, (int, float)):
            dst.default_value = sock
        elif isinstance(sock, (tuple, list)):
            dst.default_value = sock
        else:
            self.L.new(sock, dst)

    def attr(self, name, dt='FLOAT'):
        nd = self.node('GeometryNodeInputNamedAttribute', data_type=dt)
        nd.inputs['Name'].default_value = name
        return nd.outputs['Attribute']

    def m(self, op, a_, b_=None, c_=None):
        nd = self.node('ShaderNodeMath', operation=op)
        self.link(a_, nd.inputs[0])
        if b_ is not None:
            self.link(b_, nd.inputs[1])
        if c_ is not None:
            self.link(c_, nd.inputs[2])
        return nd.outputs[0]

    def v(self, op, a_, b_=None, scale=None):
        nd = self.node('ShaderNodeVectorMath', operation=op)
        self.link(a_, nd.inputs[0])
        if b_ is not None:
            self.link(b_, nd.inputs[1])
        if scale is not None:
            self.link(scale, nd.inputs['Scale'])
        return nd.outputs[0]

    def cmp(self, op, a_, b_):
        nd = self.node('FunctionNodeCompare', data_type='FLOAT', operation=op)
        self.link(a_, nd.inputs['A'])
        self.link(b_, nd.inputs['B'])
        return nd.outputs['Result']


def particle_tree(kind, inst_coll=None, mat=None):
    g = G(f'FX_{kind}_eval')
    T = g.node('GeometryNodeInputSceneTime')
    t = g.m('DIVIDE', g.m('SUBTRACT', T.outputs['Frame'], g.attr('fb')), FPS)      # seconds since birth
    tc = g.m('MAXIMUM', g.m('MINIMUM', t, g.attr('th')), 0.0)
    k = g.attr('k')
    e = g.m('DIVIDE', g.m('SUBTRACT', 1.0, g.m('EXPONENT', g.m('MULTIPLY', g.m('MULTIPLY', k, -1.0), tc))), k)
    vt, v0 = g.attr('vt', 'FLOAT_VECTOR'), g.attr('v0', 'FLOAT_VECTOR')
    pos = g.node('GeometryNodeInputPosition').outputs[0]
    p = g.v('ADD', g.v('ADD', pos, g.v('SCALE', vt, scale=tc)), g.v('SCALE', g.v('SUBTRACT', v0, vt), scale=e))
    alive = g.node('FunctionNodeBooleanMath', operation='AND')
    g.link(g.cmp('GREATER_EQUAL', t, 0.0), alive.inputs[0])
    g.link(g.cmp('LESS_THAN', t, g.attr('life')), alive.inputs[1])
    dead = g.node('FunctionNodeBooleanMath', operation='NOT')
    g.link(alive.outputs[0], dead.inputs[0])
    dl = g.node('GeometryNodeDeleteGeometry', domain='POINT')
    g.link(g.gin.outputs[0], dl.inputs['Geometry'])
    g.link(dead.outputs[0], dl.inputs['Selection'])
    # velocity for Cycles motion blur (m/s): vt + (v0-vt) e^{-k tc}, zero once settled
    ek = g.m('EXPONENT', g.m('MULTIPLY', g.m('MULTIPLY', k, -1.0), tc))
    moving = g.cmp('LESS_THAN', t, g.attr('th'))
    vel = g.v('SCALE', g.v('ADD', vt, g.v('SCALE', g.v('SUBTRACT', v0, vt), scale=ek)), scale=moving)
    sp = g.node('GeometryNodeSetPosition')
    g.link(dl.outputs[0], sp.inputs['Geometry'])
    g.link(p, sp.inputs['Position'])
    sv = g.node('GeometryNodeStoreNamedAttribute', data_type='FLOAT_VECTOR', domain='POINT')
    sv.inputs['Name'].default_value = 'velocity'
    g.link(sp.outputs[0], sv.inputs['Geometry'])
    g.link(vel, sv.inputs['Value'])
    geo = sv.outputs[0]
    if kind == 'grit':
        mp = g.node('GeometryNodeMeshToPoints')
        g.link(geo, mp.inputs['Mesh'])
        g.link(g.attr('r0'), mp.inputs['Radius'])
        sm = g.node('GeometryNodeSetMaterial')
        g.link(mp.outputs[0], sm.inputs['Geometry'])
        sm.inputs['Material'].default_value = mat
        out = sm.outputs[0]
    elif kind == 'clod':
        ci = g.node('GeometryNodeCollectionInfo', transform_space='ORIGINAL')
        ci.inputs['Collection'].default_value = inst_coll
        ci.inputs['Separate Children'].default_value = True
        ci.inputs['Reset Children'].default_value = True
        rot = g.v('ADD', g.attr('rot', 'FLOAT_VECTOR'), g.v('SCALE', g.attr('spin', 'FLOAT_VECTOR'), scale=tc))
        e2r = g.node('FunctionNodeEulerToRotation')
        g.link(rot, e2r.inputs[0])
        io = g.node('GeometryNodeInstanceOnPoints')
        g.link(geo, io.inputs['Points'])
        g.link(ci.outputs[0], io.inputs['Instance'])
        io.inputs['Pick Instance'].default_value = True
        g.link(g.attr('var', 'INT'), io.inputs['Instance Index'])
        g.link(e2r.outputs[0], io.inputs['Rotation'])
        g.link(g.attr('r0'), io.inputs['Scale'])
        rl = g.node('GeometryNodeRealizeInstances')
        g.link(io.outputs[0], rl.inputs[0])
        out = rl.outputs[0]
    else:  # dust
        # radius grows fast then keeps spreading; density falls with age and with the spread
        r0 = g.attr('r0')
        tt = g.m('MAXIMUM', t, 0.0)
        grow = g.m('SUBTRACT', 1.0, g.m('EXPONENT', g.m('DIVIDE', tt, -0.9)))
        R = g.m('ADD', g.m('MULTIPLY', r0, g.m('MULTIPLY_ADD', grow, 2.6, 1.0)), g.m('MULTIPLY', tt, 0.28))
        fade_in = g.m('MINIMUM', g.m('DIVIDE', tt, 0.08), 1.0)
        dens = g.m('MULTIPLY', g.m('MULTIPLY', g.attr('d0'), g.m('EXPONENT', g.m('DIVIDE', tt, -2.4))),
                   g.m('MULTIPLY', g.m('POWER', g.m('DIVIDE', r0, R), 1.0), fade_in))
        # keep puffs off the lens: a puff that would swallow the active camera thins out (and never renders black)
        ac = g.node('GeometryNodeInputActiveCamera')
        ci_ = g.node('GeometryNodeObjectInfo', transform_space='ORIGINAL')
        g.link(ac.outputs[0], ci_.inputs['Object'])
        dn = g.node('ShaderNodeVectorMath', operation='DISTANCE')
        g.link(p, dn.inputs[0])
        g.link(ci_.outputs['Location'], dn.inputs[1])
        cam_r = g.node('ShaderNodeMapRange', interpolation_type='SMOOTHSTEP')
        g.link(g.m('SUBTRACT', dn.outputs['Value'], R), cam_r.inputs['Value'])
        cam_r.inputs['From Min'].default_value = 0.3
        cam_r.inputs['From Max'].default_value = 1.5
        dens = g.m('MULTIPLY', dens, cam_r.outputs['Result'])
        sd = g.node('GeometryNodeStoreNamedAttribute', data_type='FLOAT', domain='POINT')
        sd.inputs['Name'].default_value = 'dens'
        g.link(geo, sd.inputs['Geometry'])
        g.link(dens, sd.inputs['Value'])
        ico = g.node('GeometryNodeMeshIcoSphere')
        ico.inputs['Radius'].default_value = 1.0
        ico.inputs['Subdivisions'].default_value = 2
        sm = g.node('GeometryNodeSetMaterial')
        g.link(ico.outputs['Mesh'], sm.inputs['Geometry'])
        sm.inputs['Material'].default_value = mat
        io = g.node('GeometryNodeInstanceOnPoints')
        g.link(sd.outputs[0], io.inputs['Points'])
        g.link(sm.outputs[0], io.inputs['Instance'])
        g.link(R, io.inputs['Scale'])
        g.link(g.attr('rot', 'FLOAT_VECTOR'), io.inputs['Rotation'])
        out = io.outputs[0]
    g.link(out, g.gout.inputs[0])
    return g.t


# ---- materials -----------------------------------------------------------------------------------------------
def new_mat(name):
    m = bpy.data.materials.get(name)
    if m:
        bpy.data.materials.remove(m)
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    return m


def dirt_mat(name, col, rough):
    m = new_mat(name)
    nt = m.node_tree
    bs = nt.nodes['Principled BSDF']
    # per-clod variation: some fresh (darker, moist), some dry and dusty
    oi = nt.nodes.new('ShaderNodeObjectInfo')
    at = nt.nodes.new('ShaderNodeAttribute')
    at.attribute_type = 'GEOMETRY'
    at.attribute_name = 'r0'
    ns = nt.nodes.new('ShaderNodeTexNoise')
    ns.inputs['Scale'].default_value = 60.0
    ramp = nt.nodes.new('ShaderNodeValToRGB')
    ramp.color_ramp.elements[0].color = (*(col * 0.62), 1)
    ramp.color_ramp.elements[1].color = (*(col * 1.25), 1)
    nt.links.new(ns.outputs['Fac'], ramp.inputs[0])
    nt.links.new(ramp.outputs[0], bs.inputs['Base Color'])
    bs.inputs['Roughness'].default_value = rough
    bs.inputs['Specular IOR Level'].default_value = 0.3
    return m


def dust_mat():
    m = new_mat('FX_dust')
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    N.remove(N['Principled BSDF'])
    out = N['Material Output']
    pv = N.new('ShaderNodeVolumePrincipled')
    col = np.clip(DIRT * np.array([2.05, 1.8, 1.55]) + 0.03, 0, 0.85)   # dry dust: lighter, warm
    pv.inputs['Color'].default_value = (*col, 1)
    pv.inputs['Anisotropy'].default_value = 0.35
    pv.inputs['Absorption Color'].default_value = (*(col * 0.6), 1)
    tc = N.new('ShaderNodeTexCoord')
    ln = N.new('ShaderNodeVectorMath')
    ln.operation = 'LENGTH'
    L.new(tc.outputs['Object'], ln.inputs[0])
    fo = N.new('ShaderNodeMapRange')
    fo.interpolation_type = 'SMOOTHSTEP'
    fo.inputs[1].default_value, fo.inputs[2].default_value = 0.25, 1.0
    fo.inputs[3].default_value, fo.inputs[4].default_value = 1.0, 0.0
    L.new(ln.outputs['Value'], fo.inputs[0])
    oi = N.new('ShaderNodeObjectInfo')
    ns = N.new('ShaderNodeTexNoise')
    ns.noise_dimensions = '4D'
    ns.inputs['Scale'].default_value = 1.7
    ns.inputs['Detail'].default_value = 5.0
    ns.inputs['Roughness'].default_value = 0.6
    L.new(tc.outputs['Object'], ns.inputs['Vector'])
    wsc = N.new('ShaderNodeMath')
    wsc.operation = 'MULTIPLY'
    wsc.inputs[1].default_value = 100.0
    L.new(oi.outputs['Random'], wsc.inputs[0])
    L.new(wsc.outputs[0], ns.inputs['W'])
    nm = N.new('ShaderNodeMapRange')
    nm.inputs[1].default_value, nm.inputs[2].default_value = 0.38, 0.72
    nm.inputs[3].default_value, nm.inputs[4].default_value = 0.0, 1.6
    L.new(ns.outputs['Fac'], nm.inputs[0])
    at = N.new('ShaderNodeAttribute')
    at.attribute_type = 'INSTANCER'
    at.attribute_name = 'dens'
    m1 = N.new('ShaderNodeMath')
    m1.operation = 'MULTIPLY'
    L.new(fo.outputs[0], m1.inputs[0])
    L.new(nm.outputs[0], m1.inputs[1])
    m2 = N.new('ShaderNodeMath')
    m2.operation = 'MULTIPLY'
    L.new(m1.outputs[0], m2.inputs[0])
    L.new(at.outputs['Fac'], m2.inputs[1])
    L.new(m2.outputs[0], pv.inputs['Density'])
    L.new(pv.outputs[0], out.inputs['Volume'])
    return m


def clod_shapes(mat):
    c = bpy.data.collections.get('FX_clod_shapes')
    if c:
        for o in list(c.objects):
            bpy.data.objects.remove(o, do_unlink=True)
    else:
        c = bpy.data.collections.new('FX_clod_shapes')      # not linked to the scene: instance source only
    import bmesh
    for j in range(6):
        me = bpy.data.meshes.new(f'FX_clod{j}')
        bm = bmesh.new()
        bmesh.ops.create_icosphere(bm, subdivisions=2, radius=1.0)
        rs = np.random.default_rng(100 + j)
        sq = rs.uniform(0.55, 1.0, 3)
        for vtx in bm.verts:
            d = vtx.co.normalized()
            # lumpy, flattened, a few broken faces
            r = 1.0 + 0.22 * math.sin(3.1 * d.x + j) * math.cos(2.7 * d.y - j) + rs.normal(0, 0.07)
            vtx.co = Vector((d.x * sq[0], d.y * sq[1], d.z * sq[2])) * r
        bm.to_mesh(me)
        bm.free()
        for pl in me.polygons:
            pl.use_smooth = j % 2 == 0
        me.materials.append(mat)
        o = bpy.data.objects.new(f'FX_clod{j}', me)
        c.objects.link(o)
    return c


def points_object(name, A, tree):
    me = bpy.data.meshes.new(name)
    N = len(A['fb'])
    me.vertices.add(N)
    me.vertices.foreach_set('co', A['p0'].astype(np.float32).ravel())
    for c in ('fb', 'k', 'r0', 'd0', 'life', 'th'):
        me.attributes.new(c, 'FLOAT', 'POINT').data.foreach_set('value', A[c].astype(np.float32))
    for c in ('v0', 'vt', 'rot', 'spin'):
        me.attributes.new(c, 'FLOAT_VECTOR', 'POINT').data.foreach_set('vector', A[c].astype(np.float32).ravel())
    me.attributes.new('var', 'INT', 'POINT').data.foreach_set('value', A['var'].astype(np.int32))
    o = bpy.data.objects.new(name, me)
    FXC.objects.link(o)
    mod = o.modifiers.new('fx', 'NODES')
    mod.node_group = tree
    return o


m_clod = dirt_mat('FX_clod', DIRT * 0.85, 0.85)
m_grit = dirt_mat('FX_grit', DIRT * 1.0, 0.9)
m_dust = dust_mat()
shapes = clod_shapes(m_clod)
made = []
for kind, mat in (('clod', m_clod), ('grit', m_grit), ('dust', m_dust)):
    A = finalize(kind)
    if A is None:
        continue
    tree = particle_tree(kind, inst_coll=shapes if kind == 'clod' else None, mat=mat)
    made.append(points_object(f'FX_{kind}', A, tree).name)

# Cycles volume settings that keep thin dust cheap: biased ray-marching with coarse steps (the dust is soft)
cy = sc.cycles
for k_, v_ in (('volume_biased', True), ('volume_step_rate', 4.0), ('volume_max_steps', 128), ('volume_bounces', 0)):
    try:
        setattr(cy, k_, v_)
    except Exception as e:
        print('FX volume setting', k_, e)
for k_, v_ in (('volume_sampling', 'EQUIANGULAR'), ('volume_interpolation', 'LINEAR'), ('volume_step_rate', 1.0)):
    try:
        setattr(m_dust.cycles, k_, v_)
    except Exception as e:
        print('FX dust material setting', k_, e)


# ---- tyre marks ---------------------------------------------------------------------------------------------
BSC = max(1.0, (n + 2) / 254.0)


def tyre_marks():
    RES = 0.01
    HW = {'r': 0.068, 'f': 0.046}          # half widths (rear 120/90, front 80/100)
    PER = {'r': 0.075, 'f': 0.06}          # knob pitch along the track
    samples = []
    for w in 'fr':
        s_acc = 0.0
        for i in range(n - 1):
            if not (CON[w][i] or CON[w][i + 1]):
                continue
            a0, a1 = CPT[w][i], CPT[w][i + 1]
            seg = np.linalg.norm((a1 - a0)[:2])
            ns = max(2, int(seg / (RES * 0.8)) + 1)
            for j in range(ns):
                u = j / ns
                if (CON[w][i] if u < 0.5 else CON[w][i + 1]):
                    c = a0 * (1 - u) + a1 * u
                    f = FWD[i] * (1 - u) + FWD[i + 1] * u
                    f = np.array([f[0], f[1]]) / (np.linalg.norm(f[:2]) + 1e-9)
                    # sink with load: deeper on the landing, shallow while rolling light
                    load = 1.0
                    samples.append((w, c[0], c[1], f[0], f[1], s_acc + seg * u, i + 1 + u, load))
            s_acc += seg
    if not samples:
        return None
    S_ = np.array([s[1:] for s in samples], np.float64)
    W_ = np.array([s[0] for s in samples])
    # extra depth for a few frames after each touchdown (the tyre is loaded and the suspension compressing)
    for w, i, vz in landings:
        sel = (W_ == w) & (S_[:, 5] >= i + 1) & (S_[:, 5] < i + 1 + 12)
        S_[sel, 6] = 1.0 + min(abs(vz) / 4.0, 1.0) * (1 - (S_[sel, 5] - (i + 1)) / 12)
    pad = 0.3
    x0, y0 = S_[:, 0].min() - pad, S_[:, 1].min() - pad
    x1, y1 = S_[:, 0].max() + pad, S_[:, 1].max() + pad
    Wd, Ht = int((x1 - x0) / RES) + 1, int((y1 - y0) / RES) + 1
    depth = np.zeros((Ht, Wd), np.float32)
    lip = np.zeros((Ht, Wd), np.float32)
    born = np.full((Ht, Wd), 1e4, np.float32)
    win = int(0.14 / RES)
    oy, ox = np.mgrid[-win:win + 1, -win:win + 1]
    for (cx, cy_, fx, fy, s, fb, load), w in zip(S_, W_):
        hw, per = HW[w], PER[w]
        jc, ic = int((cx - x0) / RES), int((cy_ - y0) / RES)
        px_ = (ox + jc) * RES + x0 - cx
        py_ = (oy + ic) * RES + y0 - cy_
        al = px_ * fx + py_ * fy                     # along the track
        lat = -px_ * fy + py_ * fx                   # across
        m_ = np.abs(al) <= RES * 0.9
        if not m_.any():
            continue
        dl = np.abs(lat) / hw
        prof = np.clip(1 - dl ** 2, 0, 1) ** 0.6
        ph = ((s + al) / per + np.where(np.abs(lat) > hw * 0.45, 0.5, 0.0)) % 1.0
        knob = 0.62 + 0.38 * np.clip(1 - np.abs(ph - 0.3) / 0.22, 0, 1)
        d = prof * knob * load
        lp = np.where((dl > 1) & (dl < 1.9), np.sin(np.pi * (dl - 1) / 0.9), 0.0) * 0.55 * load
        sl = (slice(ic - win, ic + win + 1), slice(jc - win, jc + win + 1))
        if ic - win < 0 or jc - win < 0 or ic + win >= Ht or jc + win >= Wd:
            continue
        d = np.where(m_, d, 0).astype(np.float32)
        lp = np.where(m_, lp, 0).astype(np.float32)
        depth[sl] = np.maximum(depth[sl], d)
        lip[sl] = np.maximum(lip[sl], lp)
        touched = m_ & ((d > 0.01) | (lp > 0.01))
        born[sl] = np.where(touched, np.minimum(born[sl], fb), born[sl])
    lip = np.where(depth > 0.05, 0, lip)            # a later pass flattens the earlier lip
    img = bpy.data.images.get('FX_tyre_marks')
    if img:
        bpy.data.images.remove(img)
    # 8-bit RGBA keeps it at 4 B/px on the GPU: birth frame stored as frame/BSC/255 (unmarked = 1.0, never reached)
    img = bpy.data.images.new('FX_tyre_marks', Wd, Ht, alpha=True, float_buffer=False)
    img.colorspace_settings.name = 'Non-Color'
    bn = np.minimum(born / BSC, 255.0) / 255.0
    px = np.stack([depth, lip, bn, np.ones_like(depth)], -1).astype(np.float32)
    img.pixels.foreach_set(px.ravel())
    img.pack()
    print('FX tyre marks', Wd, 'x', Ht, 'px samples', len(S_), 'landings', [(w, i + 1) for w, i, _ in landings])
    return img, (x0, y0, Wd * RES, Ht * RES)


def hook_marks(img, box):
    m = bpy.data.materials.get('track_dirt_displaced')
    if m is None or m.get('fx_marks'):
        return
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    bs = next(nd for nd in N if nd.type == 'BSDF_PRINCIPLED')
    x0, y0, sx, sy = box
    tc = N.new('ShaderNodeTexCoord')
    mp = N.new('ShaderNodeMapping')
    mp.vector_type = 'POINT'
    mp.inputs['Scale'].default_value = (1 / sx, 1 / sy, 1)
    mp.inputs['Location'].default_value = (-x0 / sx, -y0 / sy, 0)
    L.new(tc.outputs['Object'], mp.inputs['Vector'])
    ti = N.new('ShaderNodeTexImage')
    ti.image = img
    ti.interpolation = 'Linear'
    ti.extension = 'CLIP'
    L.new(mp.outputs[0], ti.inputs['Vector'])
    sep = N.new('ShaderNodeSeparateColor')
    L.new(ti.outputs['Color'], sep.inputs[0])
    # current frame, by driver: marks exist only once the wheel has passed
    fv = N.new('ShaderNodeValue')
    fv.name = 'FX_frame'
    fcu = fv.outputs[0].driver_add('default_value')
    fcu.driver.type = 'SCRIPTED'
    fcu.driver.expression = 'frame'
    bfr = N.new('ShaderNodeMath')
    bfr.operation = 'MULTIPLY'
    bfr.inputs[1].default_value = 255.0 * BSC
    L.new(sep.outputs[2], bfr.inputs[0])
    gate = N.new('ShaderNodeMath')
    gate.operation = 'LESS_THAN'
    L.new(bfr.outputs[0], gate.inputs[0])
    L.new(fv.outputs[0], gate.inputs[1])
    dep = N.new('ShaderNodeMath')
    dep.operation = 'MULTIPLY'
    L.new(sep.outputs[0], dep.inputs[0])
    L.new(gate.outputs[0], dep.inputs[1])
    lp = N.new('ShaderNodeMath')
    lp.operation = 'MULTIPLY'
    L.new(sep.outputs[1], lp.inputs[0])
    L.new(gate.outputs[0], lp.inputs[1])
    # colour: compressed, moister dirt darker; lip of loose dirt slightly lighter
    bc_src = bs.inputs['Base Color'].links[0].from_socket
    dk = N.new('ShaderNodeMix')
    dk.data_type = 'RGBA'
    dk.blend_type = 'MULTIPLY'
    dk.inputs[7].default_value = (0.52, 0.47, 0.43, 1)
    dkf = N.new('ShaderNodeMath')
    dkf.operation = 'MULTIPLY'
    dkf.use_clamp = True
    dkf.inputs[1].default_value = 1.1
    L.new(dep.outputs[0], dkf.inputs[0])
    L.new(dkf.outputs[0], dk.inputs['Factor'])
    L.new(bc_src, dk.inputs[6])
    lt = N.new('ShaderNodeMix')
    lt.data_type = 'RGBA'
    lt.blend_type = 'SCREEN'
    lt.inputs[7].default_value = (0.3, 0.26, 0.22, 1)
    L.new(lp.outputs[0], lt.inputs['Factor'])
    L.new(dk.outputs[2], lt.inputs[6])
    L.new(lt.outputs[2], bs.inputs['Base Color'])
    # roughness: packed tread a little smoother
    if bs.inputs['Roughness'].is_linked:
        rs = bs.inputs['Roughness'].links[0].from_socket
        rsub = N.new('ShaderNodeMath')
        rsub.operation = 'MULTIPLY_ADD'
        rsub.use_clamp = True
        L.new(dep.outputs[0], rsub.inputs[0])
        rsub.inputs[1].default_value = -0.18
        L.new(rs, rsub.inputs[2])
        L.new(rsub.outputs[0], bs.inputs['Roughness'])
    # relief: bump (time-gated, so the displaced terrain is never re-diced per frame)
    h = N.new('ShaderNodeMath')
    h.operation = 'MULTIPLY_ADD'
    L.new(dep.outputs[0], h.inputs[0])
    h.inputs[1].default_value = -0.028
    h2 = N.new('ShaderNodeMath')
    h2.operation = 'MULTIPLY'
    h2.inputs[1].default_value = 0.012
    L.new(lp.outputs[0], h2.inputs[0])
    L.new(h2.outputs[0], h.inputs[2])
    bump = N.new('ShaderNodeBump')
    bump.inputs['Strength'].default_value = 1.0
    bump.inputs['Distance'].default_value = 1.0
    L.new(h.outputs[0], bump.inputs['Height'])
    if bs.inputs['Normal'].is_linked:
        L.new(bs.inputs['Normal'].links[0].from_socket, bump.inputs['Normal'])
    L.new(bump.outputs['Normal'], bs.inputs['Normal'])
    m['fx_marks'] = 1


if 'marks' not in OFF:
    r_ = tyre_marks()
    if r_:
        hook_marks(*r_)

sc['fx_landings'] = [i + 1 for w, i, _ in landings]
sc.frame_set(sc.frame_current)
print('FX done', made, 'in', round(time.time() - _t0, 1), 's')

# exec'd from animate.py (uses sc, C, F, M, a). Cameras: CAM_follow -> CAM_trackside_low (jump) -> CAM_closeup.
P = [M @ Vector(f['pos']) for f in F]
n = len(P)


def smooth(arr, w):
    out = []
    for i in range(len(arr)):
        s = arr[max(0, i - w):i + w + 1]
        out.append(sum(s, Vector()) / len(s))
    return out


def ground_z(x, y, top=200.0):
    """Highest track surface under (x, y), ignoring bike/rider."""
    dg = bpy.context.evaluated_depsgraph_get()
    org = Vector((x, y, top))
    for _ in range(20):
        hit, loc, nrm, idx, ob, mat = sc.ray_cast(dg, org, Vector((0, 0, -1)))
        if not hit:
            return None
        if ob and any(c.name == 'track' for c in ob.users_collection):
            return loc.z
        org = loc - Vector((0, 0, 0.01))
    return None


def visible(cp, tp):
    dg = bpy.context.evaluated_depsgraph_get()
    d = tp - cp
    dist = d.length
    org = cp.copy()
    for _ in range(10):
        hit, loc, nrm, idx, ob, mat = sc.ray_cast(dg, org, d.normalized(), distance=max(0.0, dist - (org - cp).length - 1.2))
        if not hit:
            return True
        if ob and any(c.name in ('bike', 'rider', 'CINEMA') for c in ob.users_collection):
            org = loc + d.normalized() * 0.02
            continue
        return False
    return True


def score(posfn, frames):
    return sum(visible(posfn(i), Pt[i] + Vector((0, 0, 0.6))) for i in frames) / max(1, len(frames))


Ps = smooth(P, 12)
fwd = [Ps[min(i + 5, n - 1)] - Ps[max(i - 5, 0)] for i in range(n)]
fwd = [Vector((v.x, v.y, 0)).normalized() if v.length > 1e-3 else Vector((0, 1, 0)) for v in fwd]
fwd = [v.normalized() for v in smooth(fwd, 15)]
side = [Vector((-v.y, v.x, 0)) for v in fwd]

tgt = bpy.data.objects.get('CINEMA_target') or bpy.data.objects.new('CINEMA_target', None)
if tgt.name not in C.objects:
    C.objects.link(tgt)
if tgt.animation_data:
    tgt.animation_data_clear()
Pt = smooth(P, 2)
for i in range(n):
    tgt.location = Pt[i] + Vector((0, 0, 0.75))
    tgt.keyframe_insert('location', frame=i + 1)


def cam(name, lens, fstop):
    o = bpy.data.objects.get(name)
    if o is None:
        o = bpy.data.objects.new(name, bpy.data.cameras.new(name))
        C.objects.link(o)
    cd = o.data
    cd.lens = lens
    cd.sensor_width = 36
    cd.clip_start = 0.1
    cd.clip_end = 3000
    cd.dof.use_dof = True
    cd.dof.focus_object = tgt
    cd.dof.aperture_fstop = fstop
    o.parent = None
    if o.animation_data:
        o.animation_data_clear()
    for c in list(o.constraints):
        o.constraints.remove(c)
    tc = o.constraints.new('TRACK_TO')
    tc.name = 'track'
    tc.target = tgt
    tc.track_axis = 'TRACK_NEGATIVE_Z'
    tc.up_axis = 'UP_Y'
    return o


air = [f.get('airborne', False) for f in F]
# the long jump: the longest airborne run
runs, s0 = [], None
for i, v in enumerate(air + [False]):
    if v and s0 is None:
        s0 = i
    if not v and s0 is not None:
        runs.append((s0, i - 1))
        s0 = None
take, land = max(runs, key=lambda r: r[1] - r[0]) if runs else (n // 3, 2 * n // 3)
pk = max(range(take, land + 1), key=lambda i: P[i].z)
sc['cinema_apex_frame'] = pk + 1

# follow: chase cam behind and to the left, smoothed, 1.3 m over the bike
fc = cam('CAM_follow', 30, 5.6)
for i in range(n):
    p = Ps[i] - fwd[i] * 5.5 + side[i] * 2.0
    gz = ground_z(p.x, p.y)
    p.z = max(Ps[i].z + 1.3, (gz or -1e9) + 1.0)
    fc.location = p
    fc.keyframe_insert('location', frame=i + 1)

# low trackside: on the ground beside the landing, wide lens looking up at the bike in the air
tsc = cam('CAM_trackside_low', 28, 8.0)
cut1 = max(2, take + 4)
cut2 = min(n - 30, land - 10)
seg = list(range(cut1 - 1, cut2, 4)) or [min(n - 1, max(0, pk))]
best = None
for sgn in (1, -1):
    for d in (7, 10, 14, 20, 28):
        for t in (-30, -20, -10, 0, 10, 20, 30):
            b = P[pk] + side[pk] * (sgn * d) + fwd[pk] * t
            g = ground_z(b.x, b.y)
            if g is None:
                continue
            b.z = g + 0.4
            sc_ = score(lambda i, b=b: b, seg)
            md = sorted((P[i] - b).length for i in seg)[len(seg) // 2]
            key = (min(round(sc_, 2), 0.9), -md)
            if best is None or key > best[0]:
                best = (key, b.copy(), md)
if best is None:   # nowhere on the ground near the apex to stand: a camera beside it, off the ground
    best = ((0, 0), P[pk] + side[pk] * 10 + Vector((0, 0, 1.5)), 10.0)
tsc.location = best[1]
# long lens sized so the bike is ~1/5 of the frame width at the median distance
# zoom with the bike: lens keyed per frame so the bike stays ~1/4 of the frame width
_ls = [max(28.0, min(200.0, 36.0 * (P[i] - tsc.location).length / 8.5)) for i in range(n)]
_ls = [sum(_ls[max(0, i - 8):i + 9]) / len(_ls[max(0, i - 8):i + 9]) for i in range(n)]
if tsc.data.animation_data:
    tsc.data.animation_data_clear()
for i in range(n):
    tsc.data.lens = _ls[i]
    tsc.data.keyframe_insert('lens', frame=i + 1)
for i in range(pk, cut2):
    if not visible(tsc.location, Pt[i] + Vector((0, 0, 0.6))) or not visible(tsc.location, Pt[i] + Vector((0, 0, 0.1))):
        cut2 = max(pk + 5, i - 2)
        break
print('CAMS trackside score', best[0], 'lens', round(tsc.data.lens))

# closeup: long lens, travelling beside the bike after the landing
cc = cam('CAM_closeup', 85, 2.8)
seg2 = list(range(cut2 - 1, n, 3))
best = None
for sgn in (-1, 1):
    for d in (10, 14, 18, 24):
        for t in (0, 6):
            def pos(i, sgn=sgn, d=d, t=t):
                p = Ps[i] + side[i] * (sgn * d) + fwd[i] * t
                g = ground_z(p.x, p.y)
                p.z = max(Ps[i].z + 0.6, (g if g is not None else -1e9) + 1.2)
                return p
            sc_ = score(pos, seg2)
            key = (min(round(sc_, 2), 0.95), -d)
            if best is None or key > best[0]:
                best = (key, pos)
print('CAMS closeup score', best[0])
for i in range(n):
    cc.location = best[1](i)
    cc.keyframe_insert('location', frame=i + 1)

mk = sc.timeline_markers
for m in list(mk):
    mk.remove(m)
for fr, o in ((1, fc), (cut1, tsc), (cut2, cc)):
    m = mk.new(f'cut{fr}', frame=fr)
    m.camera = o
sc.camera = fc
print('CAMS take', take + 1, 'apex', pk + 1, 'land', land + 1, 'cuts', cut1, cut2, 'trackside at', tuple(round(v, 1) for v in tsc.location))

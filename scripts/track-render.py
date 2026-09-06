#!/usr/bin/env python3
"""Draw a compiled track: the ground from its heightfield, the objects from its .map.

The picture is the check. Numbers say a fence is 19 m from the centreline and say nothing
about whether it is standing in a jump face — and the corpus work found two placement faults
that every measurement passed. So `trackscenery::built::builds_a_track_with_objects` writes a
`scene.bin` beside the compiled track and this draws it:

    FROST_BUILD=/tmp/objtrack ... cargo test -- --ignored --nocapture builds_a_track_with_objects
    python3 scripts/track-render.py /tmp/objtrack/scene.bin /tmp/shot.png

Three views come out: what a rider sees, the same corner from beside the track, and the lap
from above. Terrain is ray-marched over the heightfield; the scenery is rasterised from the
`.map`'s own triangles through its own sheets, with the alpha test that makes a foliage card
a tree rather than a green slab. Needs numpy and pillow, nothing else."""
import struct, sys, math
import numpy as np
from PIL import Image

def load(path):
    b = open(path, 'rb').read()
    o = 0
    assert b[:4] == b'SDMP', b[:4]
    o = 4
    gw, gh = struct.unpack_from('<II', b, o); o += 8
    mps, size_x = struct.unpack_from('<ff', b, o); o += 8
    n = gw * gh
    H = np.frombuffer(b, '<f4', n, o).reshape(gh, gw).copy(); o += 4 * n
    C = np.frombuffer(b, 'u1', n, o).reshape(gh, gw).copy(); o += n
    nm, = struct.unpack_from('<I', b, o); o += 4
    mats = []
    for _ in range(nm):
        dim, = struct.unpack_from('<I', b, o); o += 4
        t = np.frombuffer(b, 'u1', dim*dim*4, o).reshape(dim, dim, 4).copy(); o += dim*dim*4
        mats.append(t)
    vc, = struct.unpack_from('<I', b, o); o += 4
    P = np.frombuffer(b, '<f4', vc*3, o).reshape(vc, 3).copy(); o += 12*vc
    UV = np.frombuffer(b, '<f4', vc*2, o).reshape(vc, 2).copy(); o += 8*vc
    tc, = struct.unpack_from('<I', b, o); o += 4
    I = np.frombuffer(b, '<u4', tc*3, o).reshape(tc, 3).copy(); o += 12*tc
    M = np.frombuffer(b, '<u4', tc, o).copy(); o += 4*tc
    return dict(gw=gw, gh=gh, mps=mps, size_x=size_x, H=H, C=C, mats=mats, P=P, UV=UV, I=I, M=M)

def sample_h(H, mps, x, z):
    gh, gw = H.shape
    fx = np.clip(x / mps, 0, gw - 1.001)
    fz = np.clip(z / mps, 0, gh - 1.001)
    x0 = fx.astype(np.int32); z0 = fz.astype(np.int32)
    tx = fx - x0; tz = fz - z0
    a = H[z0, x0]; bb = H[z0, x0+1]; c = H[z0+1, x0]; d = H[z0+1, x0+1]
    return (a*(1-tx)+bb*tx)*(1-tz) + (c*(1-tx)+d*tx)*tz

def look_at(eye, target, up=(0,1,0)):
    f = np.array(target, np.float64) - np.array(eye, np.float64)
    f /= np.linalg.norm(f)
    u = np.array(up, np.float64)
    s = np.cross(f, u); s /= np.linalg.norm(s)
    u = np.cross(s, f)
    return np.stack([s, u, f])           # rows: right, up, forward

def render(d, eye, target, W=1100, Hh=680, fov=58.0, sky=(150,178,205)):
    H, mps = d['H'], d['mps']
    R = look_at(eye, target)
    eye = np.array(eye, np.float64)
    ar = W / Hh
    th = math.tan(math.radians(fov) * 0.5)
    px = (np.arange(W) + 0.5) / W * 2 - 1
    py = 1 - (np.arange(Hh) + 0.5) / Hh * 2
    PX, PY = np.meshgrid(px, py)
    dirs = (R[0] * (PX * th * ar)[..., None] + R[1] * (PY * th)[..., None] + R[2])
    dirs /= np.linalg.norm(dirs, axis=-1, keepdims=True)

    # --- terrain, by marching the heightfield ---
    far = float(max(d['size_x'], 400.0)) * 1.6
    t = np.full((Hh, W), 1.0)
    hit = np.zeros((Hh, W), bool)
    step0 = 0.35
    tt = t.copy()
    prev = np.zeros((Hh, W))
    for i in range(900):
        step = step0 * (1.0 + tt * 0.02)
        cand = tt + step
        p = eye + dirs * cand[..., None]
        h = sample_h(H, mps, p[..., 0], p[..., 2])
        under = (p[..., 1] < h) & ~hit & (cand < far)
        prev = np.where(~hit, tt, prev)
        hit |= under
        tt = np.where(hit, tt, cand)
        if hit.all() or tt.min() > far:
            break
    # refine
    lo, hi = prev, tt
    for _ in range(24):
        mid = 0.5 * (lo + hi)
        p = eye + dirs * mid[..., None]
        h = sample_h(H, mps, p[..., 0], p[..., 2])
        below = p[..., 1] < h
        hi = np.where(below, mid, hi)
        lo = np.where(below, lo, mid)
    tdepth = np.where(hit, hi, np.inf)
    ph = eye + dirs * np.where(hit, hi, 0)[..., None]

    # ground shading: slope + the corridor tint
    gx = np.clip(ph[..., 0] / mps, 1, H.shape[1]-2).astype(np.int32)
    gz = np.clip(ph[..., 2] / mps, 1, H.shape[0]-2).astype(np.int32)
    dzdx = (H[gz, gx+1] - H[gz, gx-1]) / (2*mps)
    dzdz = (H[gz+1, gx] - H[gz-1, gx]) / (2*mps)
    n = np.stack([-dzdx, np.ones_like(dzdx), -dzdz], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    L = np.array([0.35, 0.86, -0.37]); L /= np.linalg.norm(L)
    lam = np.clip((n * L).sum(-1), 0, 1)
    on = d['C'][gz, gx] > 0
    base = np.where(on[..., None], np.array([0.42, 0.30, 0.22]), np.array([0.34, 0.40, 0.22]))
    col = base * (0.42 + 0.72 * lam)[..., None]
    img = np.where(hit[..., None], col, np.array(sky)/255.0)

    # --- scenery, rasterised over the same depth buffer ---
    P, UV, I, M, mats = d['P'], d['UV'], d['I'], d['M'], d['mats']
    if len(I):
        rel = P.astype(np.float64) - eye
        cam = rel @ R.T                                # x right, y up, z forward
        zc = cam[:, 2]
        sx = (cam[:, 0] / (zc * th * ar) * 0.5 + 0.5) * W
        sy = (0.5 - cam[:, 1] / (zc * th) * 0.5) * Hh
        order = np.argsort(-zc[I].mean(1))             # far to near; the z-buffer settles ties
        for tri in order:
            a, bq, c = I[tri]
            if zc[a] <= 0.2 or zc[bq] <= 0.2 or zc[c] <= 0.2:
                continue
            xs = np.array([sx[a], sx[bq], sx[c]]); ys = np.array([sy[a], sy[bq], sy[c]])
            x0 = max(int(np.floor(xs.min())), 0); x1 = min(int(np.ceil(xs.max())) + 1, W)
            y0 = max(int(np.floor(ys.min())), 0); y1 = min(int(np.ceil(ys.max())) + 1, Hh)
            if x0 >= x1 or y0 >= y1 or (x1-x0)*(y1-y0) > 400000:
                continue
            X, Y = np.meshgrid(np.arange(x0, x1) + 0.5, np.arange(y0, y1) + 0.5)
            d0 = (xs[1]-xs[0])*(ys[2]-ys[0]) - (xs[2]-xs[0])*(ys[1]-ys[0])
            if abs(d0) < 1e-9:
                continue
            w1 = ((X-xs[0])*(ys[2]-ys[0]) - (Y-ys[0])*(xs[2]-xs[0])) / d0
            w2 = ((Y-ys[0])*(xs[1]-xs[0]) - (X-xs[0])*(ys[1]-ys[0])) / d0
            w0 = 1 - w1 - w2
            inside = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
            if not inside.any():
                continue
            # perspective-correct interpolation
            iz = w0/zc[a] + w1/zc[bq] + w2/zc[c]
            depth = 1.0 / np.maximum(iz, 1e-9)
            vis = inside & (depth < tdepth[y0:y1, x0:x1])
            if not vis.any():
                continue
            u = (w0*UV[a,0]/zc[a] + w1*UV[bq,0]/zc[bq] + w2*UV[c,0]/zc[c]) * depth
            v = (w0*UV[a,1]/zc[a] + w1*UV[bq,1]/zc[bq] + w2*UV[c,1]/zc[c]) * depth
            tex = mats[M[tri] % len(mats)]
            dim = tex.shape[0]
            ti = np.clip((np.mod(u, 1.0) * dim).astype(np.int32), 0, dim-1)
            tj = np.clip((np.mod(v, 1.0) * dim).astype(np.int32), 0, dim-1)
            texel = tex[tj, ti]
            vis &= texel[..., 3] > 110
            if not vis.any():
                continue
            # flat shade from the triangle's own normal
            e1 = P[bq] - P[a]; e2 = P[c] - P[a]
            nn = np.cross(e1, e2); ln = np.linalg.norm(nn)
            lam2 = 0.55 if ln < 1e-9 else 0.45 + 0.55*abs(float(np.dot(nn/ln, L)))
            rgb = texel[..., :3].astype(np.float64) / 255.0 * lam2
            sub = img[y0:y1, x0:x1]
            sub[vis] = rgb[vis]
            img[y0:y1, x0:x1] = sub
            sd = tdepth[y0:y1, x0:x1]
            sd[vis] = depth[vis]
            tdepth[y0:y1, x0:x1] = sd

    # distance haze
    fog = np.clip((tdepth - 90.0) / 700.0, 0, 0.72)[..., None]
    img = img * (1 - fog) + np.array(sky)/255.0 * fog
    return (np.clip(img, 0, 1) * 255).astype(np.uint8)

if __name__ == '__main__':
    d = load(sys.argv[1])
    print('grid', d['gw'], d['gh'], 'mps %.3f' % d['mps'],
          'scenery', len(d['P']), 'verts', len(d['I']), 'tris', len(d['mats']), 'sheets')
    # A point on the riding line, and which way it runs there.
    zs, xs = np.nonzero(d['C'])
    k = len(zs) // 3
    cx, cz = xs[k] * d['mps'], zs[k] * d['mps']
    near = (np.abs(xs*d['mps'] - cx) < 30) & (np.abs(zs*d['mps'] - cz) < 30)
    pts = np.stack([xs[near]*d['mps'] - cx, zs[near]*d['mps'] - cz], 1)
    u, s, vt = np.linalg.svd(pts - pts.mean(0), full_matrices=False)
    dirv = vt[0]
    gy = float(sample_h(d['H'], d['mps'], np.array([cx]), np.array([cz]))[0])

    views = {
      'rider': (( cx - dirv[0]*22, gy + 2.4, cz - dirv[1]*22),
                ( cx + dirv[0]*60, gy + 1.0, cz + dirv[1]*60), 62.0),
      'trackside': (( cx - dirv[1]*34, gy + 9.0, cz + dirv[0]*34),
                    ( cx + dirv[0]*25, gy + 1.0, cz + dirv[1]*25), 55.0),
      'aerial': ((d['size_x']*0.5 - 210, gy + 210, d['size_x']*0.5 - 250),
                 (d['size_x']*0.5, gy + 2, d['size_x']*0.5), 60.0),
    }
    for name, (eye, tgt, fov) in views.items():
        print('rendering', name, '...', flush=True)
        im = render(d, eye, tgt, fov=fov)
        out = sys.argv[2].replace('.png', f'-{name}.png')
        Image.fromarray(im).save(out)
        print('  wrote', out)

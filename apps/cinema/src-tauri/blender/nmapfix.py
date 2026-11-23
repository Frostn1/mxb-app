# MXB Cinema: read every *_n normal map the way it was authored, measured on the built meshes.
#
#   from nmapfix import fix_normal_maps
#   report = fix_normal_maps(bpy.data.collections['bike'].all_objects)
#
# The same test mxb-blender-kit's bake/convention.py uses on .edf files: on loops whose split normal bends
# away from the face normal (smoothed edges), a correctly read map pulls the shading normal back toward the
# real face. Each map is scored as-is and with its green, red, or red+green channels flipped in the mesh's own
# Blender tangent frame; the best reading wins when it beats as-is by a clear margin. Read the wrong way a map
# shades edges as if they were bevelled inward (Sean: "handguards look painted inward"). On the KTM 250 SX-F the
# main atlas (dgsbike_n) reads right with green flipped (0.815 vs 0.793 as-is over 348k loops).
import numpy as np

MARGIN = 0.008
READINGS = {'as-is': (1, 1), 'flipG': (1, -1), 'flipR': (-1, 1), 'flipRG': (-1, -1)}


def _pixels(im, cache):
    if im.name not in cache:
        w, h = im.size
        px = np.empty(w * h * 4, np.float32)
        im.pixels.foreach_get(px)
        cache[im.name] = px.reshape(h, w, 4)
    return cache[im.name]


def measure(objects):
    scores, cache = {}, {}
    for ob in objects:
        if ob.type != 'MESH' or not ob.data.uv_layers or not ob.data.materials:
            continue
        me = ob.data
        try:
            me.calc_tangents(uvmap=me.uv_layers[0].name)
        except Exception:
            continue
        nl = len(me.loops)
        T = np.empty(nl * 3); me.loops.foreach_get('tangent', T); T = T.reshape(-1, 3)
        S = np.empty(nl); me.loops.foreach_get('bitangent_sign', S)
        NL = np.empty(nl * 3); me.loops.foreach_get('normal', NL); NL = NL.reshape(-1, 3)
        UV = np.empty(nl * 2); me.uv_layers[0].data.foreach_get('uv', UV); UV = UV.reshape(-1, 2)
        npoly = len(me.polygons)
        FN = np.empty(npoly * 3); me.polygons.foreach_get('normal', FN); FN = FN.reshape(-1, 3)
        LT = np.empty(npoly, np.int64); me.polygons.foreach_get('loop_total', LT)
        MI = np.empty(npoly, np.int64); me.polygons.foreach_get('material_index', MI)
        poly_of = np.repeat(np.arange(npoly), LT)
        fn = FN[poly_of]
        bent = np.einsum('ij,ij->i', NL, fn) < 0.97
        for mi, mat in enumerate(me.materials):
            if not mat or not mat.node_tree:
                continue
            nm = next((n for n in mat.node_tree.nodes if n.type == 'NORMAL_MAP'), None)
            if not nm or not nm.inputs['Color'].links:
                continue
            src = nm.inputs['Color'].links[0].from_node
            im = getattr(src, 'image', None)
            if im is None:
                continue
            sel = bent & (MI[poly_of] == mi)
            if sel.sum() < 50:
                continue
            a = _pixels(im, cache); h, w = a.shape[:2]
            uv = UV[sel]
            x = (uv[:, 0] % 1 * w).astype(int).clip(0, w - 1); y = (uv[:, 1] % 1 * h).astype(int).clip(0, h - 1)
            c = a[y, x, :3] * 2 - 1
            t = T[sel]; n = NL[sel]; b = S[sel][:, None] * np.cross(n, t); f = fn[sel]
            for k, (sr, sg) in READINGS.items():
                m = sr * c[:, :1] * t + sg * c[:, 1:2] * b + c[:, 2:3] * n
                m /= np.linalg.norm(m, axis=1, keepdims=True) + 1e-9
                d = scores.setdefault(im.name, {}).setdefault(k, [0.0, 0])
                d[0] += float(np.einsum('ij,ij->i', m, f).sum()); d[1] += int(sel.sum())
    return {name: {k: s / max(1, n) for k, (s, n) in v.items()} for name, v in scores.items()}


def _insert_flip(mat, sr, sg):
    nt = mat.node_tree; N, L = nt.nodes, nt.links
    for nm in [n for n in N if n.type == 'NORMAL_MAP' and n.inputs['Color'].links]:
        link = nm.inputs['Color'].links[0]
        src = link.from_socket
        if src.node.type == 'COMBINE_COLOR' and src.node.get('cinema_nflip'):
            continue
        sep = N.new('ShaderNodeSeparateColor'); com = N.new('ShaderNodeCombineColor'); com['cinema_nflip'] = 1
        L.new(src, sep.inputs[0])
        for i, sign in ((0, sr), (1, sg)):
            if sign < 0:
                inv = N.new('ShaderNodeMath'); inv.operation = 'SUBTRACT'; inv.inputs[0].default_value = 1.0
                L.new(sep.outputs[i], inv.inputs[1]); L.new(inv.outputs[0], com.inputs[i])
            else:
                L.new(sep.outputs[i], com.inputs[i])
        L.new(sep.outputs[2], com.inputs[2])
        L.new(com.outputs[0], nm.inputs['Color'])


def fix_normal_maps(objects, log=print):
    objects = list(objects)
    scores = measure(objects)
    choice = {}
    for name, sc in scores.items():
        best = max(sc, key=sc.get)
        choice[name] = best if sc[best] - sc['as-is'] > MARGIN else 'as-is'
    done = set()
    for ob in objects:
        if ob.type != 'MESH':
            continue
        for mat in ob.data.materials:
            if not mat or not mat.node_tree or mat.name in done:
                continue
            nm = next((n for n in mat.node_tree.nodes if n.type == 'NORMAL_MAP' and n.inputs['Color'].links), None)
            im = getattr(nm.inputs['Color'].links[0].from_node, 'image', None) if nm else None
            if im is None or choice.get(im.name, 'as-is') == 'as-is':
                continue
            _insert_flip(mat, *READINGS[choice[im.name]])
            done.add(mat.name)
    for name in sorted(scores):
        log('nmap', name, choice[name], {k: round(v, 4) for k, v in scores[name].items()})
    return choice

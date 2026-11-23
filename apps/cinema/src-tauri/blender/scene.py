# MXB Cinema: build the scene (track + bike + rider + lighting + cameras) from what the app prepared.
# Run by cinema_run.py inside the one Blender process; the inputs are the job's folders:
#   track_dir  terrain grid, .map scenery and sheets, ground layers, .amb light, sky picture (prep.rs track)
#   bike_dir   the assembled bike as the app's viewer reads it: nodes + rig + sheets + .geom/gfx.cfg (prep.rs bike)
#   rider_dir  body mesh, bones and skin as authored; outfit; gear bump maps (prep.rs rider), or none
# Axes: game (X right, Y up, Z fwd, left-handed) -> Blender (-x, -z, y), same as mxb-blender-kit.
import bpy, json, math, os, base64, shutil
import numpy as np
from mathutils import Matrix, Vector
from cinema_job import JOB, OUT_TEX, log

TD = JOB['track_dir']
BD = JOB['bike_dir']
RD = JOB.get('rider_dir')
M3 = Matrix(((-1, 0, 0), (0, 0, -1), (0, 1, 0)))
M4 = M3.to_4x4()


def g2b(v):
    return Vector((-v[0], -v[2], v[1]))


def game_xform(rot_deg, off):
    """Game-frame affine (rotate about game X, then translate) -> Blender matrix."""
    r = Matrix.Rotation(math.radians(rot_deg), 4, 'X')
    t = Matrix.Translation(Vector(off)) @ r
    return M4 @ t @ M4.transposed()


def coll(name, parent=None):
    c = bpy.data.collections.get(name) or bpy.data.collections.new(name)
    p = parent or bpy.context.scene.collection
    if c.name not in [x.name for x in p.children]:
        p.children.link(c)
    return c


def img_from_raw(name, path, w, h, ch=4, non_color=False, flipv=False, max_dim=0):
    """A raw RGBA (or one-channel) sheet as an image in the shot's own textures folder.
    The PNG is made once beside the raw file in the cache and copied for each shot, so the
    .blend opens on its own wherever the cache goes."""
    safe = "".join(c if c.isalnum() or c in "-_." else "_" for c in name)
    cached_png = path + ".png"
    if not os.path.exists(cached_png):
        a = np.fromfile(path, np.uint8).reshape(h, w, ch)
        if max_dim and w > max_dim:
            k = w // max_dim
            a = a[::k, ::k]
            h, w = a.shape[:2]
        a = a.astype(np.float32) / 255.0
        if ch == 1:
            a = np.repeat(a, 4, axis=2); a[..., 3] = 1.0
        if not flipv:
            a = a[::-1]  # file rows top-first -> Blender rows bottom-first
        im = bpy.data.images.new("tmp_" + safe, w, h, alpha=True)
        im.pixels.foreach_set(a.ravel())
        im.filepath_raw = cached_png
        im.file_format = 'PNG'
        im.save()
        bpy.data.images.remove(im)
    dst = os.path.join(OUT_TEX, safe + ".png")
    if not os.path.exists(dst):
        shutil.copyfile(cached_png, dst)
    im = bpy.data.images.load(dst, check_existing=True)
    im.name = safe
    if non_color:
        im.colorspace_settings.name = 'Non-Color'
    return im


# ---------------------------------------------------------------- reset: an empty scene, the GPU prefs kept
sc = bpy.context.scene
for o in list(bpy.data.objects):
    bpy.data.objects.remove(o, do_unlink=True)
for c in list(bpy.data.collections):
    bpy.data.collections.remove(c)
for blk in (bpy.data.meshes, bpy.data.materials, bpy.data.cameras, bpy.data.lights):
    for x in list(blk):
        blk.remove(x)
C_TRACK, C_BIKE, C_RIDER, C_LIGHT, C_CAM = [coll(n) for n in ("track", "bike", "rider", "lighting", "cameras")]

# ---------------------------------------------------------------- terrain
tj = json.load(open(os.path.join(TD, "track.json")))
n_, mps = tj["w"], tj["mps"]
n = n_
H = np.fromfile(os.path.join(TD, "terrain.f32"), np.float32).reshape(tj["h"], n)  # [row=z, col=x]
cols, rows = np.meshgrid(np.arange(n), np.arange(tj["h"]))
gx, gz = cols * mps, rows * mps
verts = np.stack([-gx, -gz, H], -1).reshape(-1, 3).astype(np.float32)
q = (rows[:-1, :-1] * n + cols[:-1, :-1]).ravel()
# winding chosen so normals point up after the mirror
quads_all = np.stack([q, q + n, q + n + 1, q + 1], -1).astype(np.int32)
# Displacement patch: the ground the clip crosses (game x/z box around its path, +margin). It gets true
# (adaptive) displacement; the rest of the terrain keeps bump-only relief.
CLIP = json.load(open(JOB['clip']))
px0, px1, pz0, pz1 = 1e9, -1e9, 1e9, -1e9
for fr_ in CLIP["riders"][0]["frames"]:
    px0, px1 = min(px0, fr_["pos"][0]), max(px1, fr_["pos"][0])
    pz0, pz1 = min(pz0, fr_["pos"][2]), max(pz1, fr_["pos"][2])
MARGIN = 14.0
qc, qr = cols[:-1, :-1].ravel() * mps, rows[:-1, :-1].ravel() * mps
in_patch = (qc >= px0 - MARGIN) & (qc <= px1 + MARGIN) & (qr >= pz0 - MARGIN) & (qr <= pz1 + MARGIN)


def grid_mesh(name, quads):
    used, inv = np.unique(quads.ravel(), return_inverse=True)
    me = bpy.data.meshes.new(name)
    me.vertices.add(len(used)); me.vertices.foreach_set("co", verts[used].ravel())
    me.loops.add(quads.size); me.loops.foreach_set("vertex_index", inv.astype(np.int32))
    me.polygons.add(len(quads))
    me.polygons.foreach_set("loop_start", np.arange(0, quads.size, 4, dtype=np.int32))
    me.polygons.foreach_set("loop_total", np.full(len(quads), 4, np.int32))
    uv = me.uv_layers.new(name="UVMap")
    lv = quads.ravel()
    uvs = np.stack([cols.ravel()[lv] / (n - 1), rows.ravel()[lv] / (tj["h"] - 1)], -1).astype(np.float32)
    uv.data.foreach_set("uv", uvs.ravel())
    me.update(calc_edges=True); me.validate()
    me.polygons.foreach_set("use_smooth", np.ones(len(quads), bool))
    if len(me.polygons) and me.polygons[0].normal.z < 0:
        me.flip_normals()
    ob = bpy.data.objects.new(name, me); C_TRACK.objects.link(ob)
    return ob


terrain = grid_mesh("track_terrain", quads_all[~in_patch])
terrain_patch = grid_mesh("track_terrain_detail", quads_all[in_patch])
sm = terrain_patch.modifiers.new("adaptive", 'SUBSURF')
try:
    sm.subdivision_type = 'SIMPLE'
except Exception:
    pass
sm.levels = 0; sm.render_levels = 4
try:
    sm.use_adaptive_subdivision = True
    sm.adaptive_pixel_size = 1.5
except Exception:  # Blender 4.x: per object, and only with the experimental feature set
    terrain_patch.cycles.use_adaptive_subdivision = True
    terrain_patch.cycles.dicing_rate = 1.5
    sc.cycles.feature_set = 'EXPERIMENTAL'
terrain_patch["note"] = "true displacement patch around the clip's path (adaptive subdivision, 1 px dicing)"
del verts, quads_all, cols, rows, gx, gz
log("terrain", n, "x", tj["h"], "at", round(mps, 4), "m")

# terrain material: the track's own layer stack, blended as the game does (ground.py); relief/roughness on top
import sys as _sys
if os.path.dirname(os.path.abspath(__file__)) not in _sys.path:
    _sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ground import ground_materials
TEXDIR = os.path.join(TD, "tex")
mat, mat_d = ground_materials(tj, TEXDIR, lambda name, path, w, h, ch=4, non_color=False, flipv=False:
                              img_from_raw(name, path, w, h, ch=ch, non_color=non_color, flipv=flipv, max_dim=4096))
terrain.data.materials.append(mat)
terrain_patch.data.materials.append(mat_d)

# ---------------------------------------------------------------- track scenery (.map)
P = np.fromfile(os.path.join(TD, "pos.f32"), np.float32).reshape(-1, 3)
NR = np.fromfile(os.path.join(TD, "nrm.f32"), np.float32).reshape(-1, 3)
UV = np.fromfile(os.path.join(TD, "uv.f32"), np.float32).reshape(-1, 2)
IX = np.fromfile(os.path.join(TD, "idx.u32"), np.uint32).reshape(-1, 3)
texm = {t["material"]: t for t in tj["textures"]}
# mxb-core's positional binding misses a record on Woodlinesands (checked on screen): material 16 is the tree
# geometry and wears the 8192 tree atlas the binding gives material 15.
if "woodlinesands" in tj.get("track_id", "").lower() and 15 in texm:
    texm[16] = texm[15]
import sys as _s2
if os.path.dirname(os.path.abspath(__file__)) not in _s2.path:
    _s2.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lighting import ambient_scaled
_ambf = os.path.join(TD, "track.amb")
FOLIAGE_AMB = ambient_scaled(open(_ambf, errors="ignore").read()) if os.path.exists(_ambf) else (0.4, 0.45, 0.5)
by_mat = {}
for m_id, t0, tcnt in tj["groups"]:
    by_mat.setdefault(m_id, []).append((t0, tcnt))
scenery_tris = 0
for m_id, runs in by_mat.items():
    t = texm.get(m_id)
    if t is None or t["name"].startswith("surface"):
        continue  # untextured helper geometry, or ground sheets the terrain material draws
    tris = np.concatenate([IX[a:a + c] for a, c in runs])
    if len(tris) == 0:
        continue
    used, inv = np.unique(tris.ravel(), return_inverse=True)
    v = P[used]; vb = np.stack([-v[:, 0], -v[:, 2], v[:, 1]], -1)
    nb = NR[used]; nb = np.stack([-nb[:, 0], -nb[:, 2], nb[:, 1]], -1)
    f = inv.reshape(-1, 3)  # source winding already matches after the mirror (checked vs normals)
    me = bpy.data.meshes.new(f"scenery_{m_id}")
    me.vertices.add(len(vb)); me.vertices.foreach_set("co", vb.ravel())
    me.loops.add(f.size); me.loops.foreach_set("vertex_index", f.ravel().astype(np.int32))
    me.polygons.add(len(f))
    me.polygons.foreach_set("loop_start", np.arange(0, f.size, 3, dtype=np.int32))
    me.polygons.foreach_set("loop_total", np.full(len(f), 3, np.int32))
    uvl = me.uv_layers.new(name="UVMap")
    uu = UV[used][f.ravel()].copy(); uu[:, 1] = 1.0 - uu[:, 1]
    uvl.data.foreach_set("uv", uu.ravel())
    me.update(calc_edges=True); me.validate()
    me.polygons.foreach_set("use_smooth", np.ones(len(f), bool))
    me.normals_split_custom_set_from_vertices([tuple(x) for x in nb])
    ob = bpy.data.objects.new(f"scenery_{m_id}", me); C_TRACK.objects.link(ob)
    scenery_tris += len(f)
    mt = bpy.data.materials.new(f"sc_{t['name']}"); mt.use_nodes = True
    b2 = mt.node_tree.nodes["Principled BSDF"]
    raw_path = os.path.join(TEXDIR, t["file"])
    im = img_from_raw(f"map_{m_id}_{t['name']}", raw_path, t["w"], t["h"])
    ti = mt.node_tree.nodes.new("ShaderNodeTexImage"); ti.image = im
    mt.node_tree.links.new(ti.outputs["Color"], b2.inputs["Base Color"])
    b2.inputs["Roughness"].default_value = 0.85
    b2.inputs["Specular IOR Level"].default_value = 0.3
    raw = np.fromfile(raw_path, np.uint8).reshape(-1, 4)
    has_cut = t["alpha"] or (raw[:, 3] < 128).mean() > 0.02
    if has_cut:
        nt2 = mt.node_tree; NN = nt2.nodes; LL = nt2.links
        out2 = NN["Material Output"]
        # hard alpha test like the game (no grey fringes, no transparency-bounce black)
        at = NN.new("ShaderNodeMath"); at.operation = 'GREATER_THAN'; at.inputs[1].default_value = 0.5
        LL.new(ti.outputs["Alpha"], at.inputs[0])
        LL.new(at.outputs[0], b2.inputs["Alpha"])  # also on the BSDF, so later alpha passes leave it alone
        op = raw[raw[:, 3] > 127][:, :3].astype(float)
        foliage = len(op) > 0 and op[:, 1].mean() > op[:, 0].mean() * 1.05 and op[:, 1].mean() > op[:, 2].mean() * 1.1
        surf = b2.outputs[0]
        if foliage:
            # leaves: light through the canopy, a little rougher
            tr = NN.new("ShaderNodeBsdfTranslucent")
            tint = NN.new("ShaderNodeMix"); tint.data_type = 'RGBA'; tint.blend_type = 'MULTIPLY'
            tint.inputs["Factor"].default_value = 1.0; tint.inputs[7].default_value = (0.75, 0.9, 0.45, 1)
            LL.new(ti.outputs["Color"], tint.inputs[6]); LL.new(tint.outputs[2], tr.inputs["Color"])
            mx2 = NN.new("ShaderNodeMixShader"); mx2.inputs[0].default_value = 0.3
            LL.new(b2.outputs[0], mx2.inputs[1]); LL.new(tr.outputs[0], mx2.inputs[2]); surf = mx2.outputs[0]
            b2.inputs["Roughness"].default_value = 0.7
            # the game lights foliage with an ambient no canopy can occlude; a dense card forest in Cycles
            # shadows itself black. Add that unoccluded ambient back, from the track's own .amb.
            em = NN.new("ShaderNodeEmission"); LL.new(ti.outputs["Color"], em.inputs["Color"])
            em.inputs["Strength"].default_value = 0.55 * max(FOLIAGE_AMB)
            emt = NN.new("ShaderNodeAddShader"); LL.new(surf, emt.inputs[0]); LL.new(em.outputs[0], emt.inputs[1]); surf = emt.outputs[0]
        tp = NN.new("ShaderNodeBsdfTransparent")
        cut = NN.new("ShaderNodeMixShader")
        LL.new(at.outputs[0], cut.inputs[0]); LL.new(tp.outputs[0], cut.inputs[1]); LL.new(surf, cut.inputs[2])
        LL.new(cut.outputs[0], out2.inputs["Surface"])
    me.materials.append(mt)
del P, NR, UV, IX
log("scenery", scenery_tris, "triangles")

# ---------------------------------------------------------------- bike: the app's assembled model on the game's joints
BJ = json.load(open(os.path.join(BD, "bike.json")))
rig = BJ["rig"]
CO = rig["chassisOrigin"]


def three_to_game(p):
    """The viewer's frame (right-handed, recentred) back to the chassis' own game frame."""
    return (CO[0] - p[0], p[1] - CO[1], p[2] - CO[2])


geom = {}
GEOMF = os.path.join(BD, "bike.geom")
if os.path.exists(GEOMF):
    for line in open(GEOMF, errors="ignore"):
        line = line.split(";")[0]
        if "=" in line:
            k, val = line.split("=", 1)
            try:
                nums = [float(x) for x in val.split(",")]
            except ValueError:
                continue
            geom[k.strip().lower()] = nums

rake = rig["rake"]
head_g = three_to_game(rig["steerHead"])
# the steer part's own origin: gfx.cfg grips are written in that frame, and it sits on the steering axis
steer_o_g = three_to_game(rig["steerOrigin"])
pivot_g = three_to_game(rig["pivot"])
fax_g = three_to_game(rig["frontAxle"]) if rig.get("frontAxle") else None
rax_g = three_to_game(rig["rearAxle"]) if rig.get("rearAxle") else None
X_STEER = game_xform(rake, steer_o_g)
X_FSUSP = game_xform(rake, steer_o_g)
X_RSUSP = game_xform(0, pivot_g)
X_FWHEEL = game_xform(0, fax_g) if fax_g else X_FSUSP
X_RWHEEL = game_xform(0, rax_g) if rax_g else X_RSUSP

bike_root = bpy.data.objects.new("CINEMA_bike_root", None); C_BIKE.objects.link(bike_root)
bike_root.empty_display_type = 'ARROWS'; bike_root.empty_display_size = 0.5
for k in ("steer", "fork", "shock", "wheel_rot_f", "wheel_rot_r"):
    bike_root[k] = 0.0


def pivot_empty(name, world, parent):
    e = bpy.data.objects.new(name, None); C_BIKE.objects.link(e)
    e.empty_display_type = 'PLAIN_AXES'; e.empty_display_size = 0.15
    e.parent = parent
    e.matrix_parent_inverse = Matrix.Identity(4)
    e.matrix_basis = world
    return e


E = {}
E["chassis"] = pivot_empty("chassis", Matrix.Identity(4), bike_root)
E["steer"] = pivot_empty("steer", X_STEER, None)
E["fsusp"] = pivot_empty("fsusp", X_FSUSP, None)
E["rsusp"] = pivot_empty("rsusp", X_RSUSP, None)
E["fwheel"] = pivot_empty("fwheel", X_FWHEEL, None)
E["rwheel"] = pivot_empty("rwheel", X_RWHEEL, None)
WORLD = {"chassis": Matrix.Identity(4), "steer": X_STEER, "fsusp": X_FSUSP, "rsusp": X_RSUSP, "fwheel": X_FWHEEL, "rwheel": X_RWHEEL}
for child, par in (("steer", "chassis"), ("fsusp", "steer"), ("rsusp", "chassis"), ("fwheel", "fsusp"), ("rwheel", "rsusp")):
    c, p = E[child], E[par]
    c.parent = p; c.matrix_parent_inverse = Matrix.Identity(4); c.matrix_basis = WORLD[par].inverted() @ WORLD[child]
bike_root["rake_deg"] = rake
bike_root["geom_head"] = list(head_g); bike_root["geom_pivot"] = list(pivot_g)
bike_root["note"] = "game node names: chassis/steer/fsusp/rsusp/fwheel/rwheel empties; steer axis = steer empty local Z through its origin; fsusp slides along it; rsusp rotates about local X"

# sheets, by name, any case
BIMG = {}
for t in BJ["textures"]:
    BIMG[t["name"].lower()] = (t, None)


def bike_img(name, non_color=False):
    if not name:
        return None
    e = BIMG.get(name.lower())
    if e is None:
        return None
    t, im = e
    if im is None:
        im = img_from_raw("bike_" + t["name"], os.path.join(BD, t["file"]), t["w"], t["h"], non_color=non_color)
        BIMG[name.lower()] = (t, im)
    return im


TYRES = {x.lower() for x in (BJ.get("tyre_textures") or [])}
METAL_HINT = ("wp7448", "xtrig", "wheel", "disc", "sprocket", "chain", "pegs", "garmin", "spoke", "rim", "hub")


def bike_material(tex):
    key = "bike_" + (tex or "untextured")
    m = bpy.data.materials.get(key)
    if m:
        return m
    m = bpy.data.materials.new(key)
    m["edf_texture"] = tex or ""
    m.use_nodes = True
    nt = m.node_tree; N = nt.nodes; L = nt.links
    for x in list(N):
        if x.type != "OUTPUT_MATERIAL":
            N.remove(x)
    b = N.new("ShaderNodeBsdfPrincipled"); L.new(b.outputs[0], N["Material Output"].inputs[0])
    uvn = N.new("ShaderNodeUVMap"); uvn.uv_map = "UVMap"
    base = tex or ""
    col = bike_img(base); nrm = bike_img(base + "_n", True); ref = bike_img(base + "_r", True)
    if col:
        t = N.new("ShaderNodeTexImage"); t.image = col; L.new(uvn.outputs[0], t.inputs[0])
        L.new(t.outputs["Color"], b.inputs["Base Color"])
    else:
        b.inputs["Base Color"].default_value = (0.05, 0.05, 0.05, 1)
    metal = any(h in base.lower() for h in METAL_HINT)
    if nrm:
        t = N.new("ShaderNodeTexImage"); t.image = nrm; L.new(uvn.outputs[0], t.inputs[0])
        nm = N.new("ShaderNodeNormalMap"); nm.uv_map = "UVMap"
        L.new(t.outputs["Color"], nm.inputs["Color"]); L.new(nm.outputs[0], b.inputs["Normal"])
        L.new(t.outputs["Alpha"], b.inputs["Specular IOR Level"])
    else:
        b.inputs["Specular IOR Level"].default_value = 0.5
    if ref:
        t = N.new("ShaderNodeTexImage"); t.image = ref; L.new(uvn.outputs[0], t.inputs[0])
        inv = N.new("ShaderNodeMapRange"); inv.inputs[3].default_value = 0.6; inv.inputs[4].default_value = 0.08
        L.new(t.outputs["Alpha"], inv.inputs[0]); L.new(inv.outputs[0], b.inputs["Roughness"])
        if metal:
            bw = N.new("ShaderNodeRGBToBW"); L.new(t.outputs["Color"], bw.inputs[0]); L.new(bw.outputs[0], b.inputs["Metallic"])
    else:
        b.inputs["Roughness"].default_value = 0.35 if metal else 0.45
        m["cinema_no_refl"] = 1     # no _r: the game shows only the sun's highlight here (tinted after the light)
    if ref and (base.lower() in ("dgsbike", "plastics") or "plastic" in base.lower()):
        # moulded plastics + decals: a clear coat only where the model gives the sheet a reflection map (_r). The
        # game mirrors no environment on a sheet without one; a coat there laid the blue sky over the orange and
        # read pink-red next to the game's plastics (F10 007/008: B/R 0.18-0.23 in Cinema vs 0.07-0.15 in game).
        b.inputs["Coat Weight"].default_value = 0.25; b.inputs["Coat Roughness"].default_value = 0.12
    if base.lower() in TYRES or "geomax" in base.lower() or "tyre" in base.lower() or "tire" in base.lower():
        b.inputs["Roughness"].default_value = 0.8; b.inputs["Specular IOR Level"].default_value = 0.2
        for l in list(b.inputs["Roughness"].links):
            nt.links.remove(l)
        m["cinema_tyre"] = 1
    m["_cinema"] = 1
    return m


# Parts whose normals/normal map are broken in the source model (see the workaround in the node loop).
BROKEN_PARTS = ("handguard",)


def bike_material_flat(tex):
    """bike_material(tex) without its normal map (for a part whose map doesn't belong to its geometry)."""
    base = bike_material(tex)
    key = base.name + "_nonmap"
    m = bpy.data.materials.get(key)
    if m:
        return m
    m = base.copy(); m.name = key
    for n in [n for n in m.node_tree.nodes if n.type == 'NORMAL_MAP']:
        for l in list(n.outputs["Normal"].links):
            m.node_tree.links.remove(l)
    return m


def group_of(name):
    nm = name.lower()
    for k in ("rwheel", "fwheel", "rsusp", "fsusp", "steer"):
        if nm.startswith(k):
            return k
    return "chassis"


def arr(s, dt):
    return np.frombuffer(base64.b64decode(s), dt) if s else np.zeros(0, dt)


bpy.context.view_layer.update()
nparts = 0
for i, nd in enumerate(BJ["nodes"]):
    P3 = arr(nd["positions"], "<f4").reshape(-1, 3).astype(np.float64)
    if not len(P3):
        continue
    N3 = arr(nd["normals"], "<f4").reshape(-1, 3).astype(np.float64)
    UV2 = arr(nd["uvs"], "<f4").reshape(-1, 2)
    I3 = arr(nd["indices"], "<u4").reshape(-1, 3)
    if not len(I3):
        continue
    # three -> Blender, in the chassis' own frame: (x - co.x, co.z - z, y - co.y)
    V = np.stack([P3[:, 0] - CO[0], CO[2] - P3[:, 2], P3[:, 1] - CO[1]], -1)
    NB = np.stack([N3[:, 0], -N3[:, 2], N3[:, 1]], -1) if len(N3) == len(P3) else None
    me = bpy.data.meshes.new(nd["name"])
    me.vertices.add(len(V)); me.vertices.foreach_set("co", V.astype(np.float32).ravel())
    me.loops.add(I3.size); me.loops.foreach_set("vertex_index", I3.ravel().astype(np.int32))
    me.polygons.add(len(I3))
    me.polygons.foreach_set("loop_start", np.arange(0, I3.size, 3, dtype=np.int32))
    me.polygons.foreach_set("loop_total", np.full(len(I3), 3, np.int32))
    if len(UV2) == len(P3):
        uvl = me.uv_layers.new(name="UVMap")
        uu = UV2[I3.ravel()].copy(); uu[:, 1] = 1.0 - uu[:, 1]
        uvl.data.foreach_set("uv", uu.ravel())
    me.update(calc_edges=True); me.validate()
    # winding: face normals should agree with the authored vertex normals
    if NB is not None and len(me.polygons):
        fn = np.empty(len(me.polygons) * 3); me.polygons.foreach_get("normal", fn); fn = fn.reshape(-1, 3)
        vn = NB[I3[:, 0]] + NB[I3[:, 1]] + NB[I3[:, 2]]
        if (np.einsum("ij,ij->i", fn, vn) < 0).mean() > 0.5:
            me.flip_normals()
    me.polygons.foreach_set("use_smooth", np.ones(len(I3), bool))
    if NB is not None:
        me.normals_split_custom_set_from_vertices([tuple(x) for x in NB])
    # materials per submesh
    subs = nd.get("submeshes") or [{"texture": nd.get("texture"), "triStart": 0, "triCount": len(I3)}]
    slots, idx = [], np.zeros(len(I3), np.int32)
    for s in subs:
        m = bike_material(s.get("texture") or nd.get("texture"))
        if m.name not in slots:
            slots.append(m.name); me.materials.append(m)
        idx[s["triStart"]:s["triStart"] + s["triCount"]] = slots.index(m.name)
    me.polygons.foreach_set("material_index", idx)
    # Source-model workaround (cinema only, Sean's files untouched): parts whose authored normals and normal
    # map are broken in the swap they came from (OB: the handguards from a 2022 model). Their faces get
    # outward normals recalculated from the geometry, smooth split normals from those, and no normal map.
    bad = [s for s in subs if any(k in (s.get("name") or "").lower() for k in BROKEN_PARTS)]
    if bad:
        import bmesh
        sel = np.zeros(len(I3), bool)
        for s in bad:
            sel[s["triStart"]:s["triStart"] + s["triCount"]] = True
            m_ok = bike_material_flat(s.get("texture") or nd.get("texture"))
            if m_ok.name not in slots:
                slots.append(m_ok.name); me.materials.append(m_ok)
            idx[s["triStart"]:s["triStart"] + s["triCount"]] = slots.index(m_ok.name)
        me.polygons.foreach_set("material_index", idx)
        bm = bmesh.new(); bm.from_mesh(me); bm.faces.ensure_lookup_table()
        bmesh.ops.recalc_face_normals(bm, faces=[bm.faces[i] for i in np.nonzero(sel)[0]])
        bm.to_mesh(me); bm.free()
        me.update()
        # loop normals: everything keeps the authored ones, the broken part gets area-weighted face normals
        nl = len(me.loops)
        fn = np.empty(len(me.polygons) * 3); me.polygons.foreach_get("normal", fn); fn = fn.reshape(-1, 3)
        ar = np.empty(len(me.polygons)); me.polygons.foreach_get("area", ar)
        lv = np.empty(nl, np.int64); me.loops.foreach_get("vertex_index", lv)
        vn = np.zeros((len(me.vertices), 3))
        fi = np.nonzero(sel)[0]
        tri_lv = lv.reshape(-1, 3)
        for k in range(3):
            np.add.at(vn, tri_lv[fi, k], fn[fi] * ar[fi, None])
        vn /= np.linalg.norm(vn, axis=1, keepdims=True) + 1e-12
        loops_sel = np.repeat(sel, 3)
        LN = NB[lv].copy() if NB is not None else vn[lv].copy()  # authored normals for everything else
        LN[loops_sel] = vn[lv[loops_sel]]
        me.normals_split_custom_set([tuple(x) for x in LN])
        log("bike part workaround (broken source normals):", [s.get("name") for s in bad])
    ob = bpy.data.objects.new(nd["name"], me); C_BIKE.objects.link(ob)
    piv = E[group_of(nd["name"])]
    ob.parent = piv
    ob.matrix_parent_inverse = WORLD[group_of(nd["name"])].inverted()
    ob["edf_object"] = nd["name"]
    if "chain" in nd["name"].lower() and not any(k in nd["name"].lower() for k in ("guide", "slider", "guard")):
        ob.hide_render = True; ob.hide_viewport = True  # the game draws the chain procedurally (gfx.cfg chain)
    nparts += 1
log("bike", BJ.get("bike"), nparts, "parts, paint", BJ.get("paint") or "stock", "tyres", BJ.get("tyres") or "own")
# read every *_n map the way it was authored (measured on these meshes; see nmapfix.py)
from nmapfix import fix_normal_maps
fix_normal_maps(C_BIKE.all_objects, log=log)


# ---- the rear linkage: shock compression (m) -> swingarm angle (rad, + lifts the axle), from the .geom
def gvec(k):
    v = geom.get(k)
    return Vector(v) if v and len(v) == 3 else None


def gnum(k):
    v = geom.get(k)
    return v[0] if v and len(v) == 1 else None


def rot_xg(p, deg):
    return Matrix.Rotation(math.radians(deg), 3, 'X') @ Vector(p)


def shock_length(deg):
    """mxb-core edf.rs shock_length: a Linkage Rod Chassis rear with the swingarm turned `deg`."""
    pv, jt = gvec("chassis_rsusp_min"), gvec("rsusp_joint")
    rs, rc, sr, shc = gvec("rocker_swingarm"), gvec("rod_chassis"), gvec("shock_rocker"), gvec("shock_chassis")
    r1, r2 = gnum("rocker_length"), gnum("rod_length")
    if None in (pv, jt, rs, rc, sr, shc, r1, r2):
        return None
    qq = pv + rot_xg(rs - jt, deg)
    qc = rc - qq
    d = qc.length
    if d < 1e-9:
        return None
    a_ = (r1 * r1 - r2 * r2 + d * d) / (2 * d)
    if r1 * r1 < a_ * a_:
        return None
    uy, uz = qc.y / d, qc.z / d
    h_ = math.sqrt(r1 * r1 - a_ * a_)
    rod_end = Vector((0.0, qq.y + a_ * uy - h_ * uz, qq.z + a_ * uz + h_ * uy))
    phi = math.degrees(math.atan2(rod_end.z - qq.z, rod_end.y - qq.y))
    ref = gnum("ref_angle")
    s_ = qq + rot_xg(sr, phi + (ref if ref is not None else -90.0))
    return (s_ - shc).length


full = (gnum("shock_minlength") or 0) + (gnum("rear_length") or 0)
table = []
if full > 0:
    for k in range(-240, 161):
        l_ = shock_length(k * 0.25)
        if l_ is not None:
            table.append((full - l_, math.radians(k * 0.25)))
    table.sort()
    mono = []
    for s_, r_ in table:
        if not mono or (s_ > mono[-1][0] + 1e-6 and r_ > mono[-1][1]):
            mono.append((s_, r_))
    table = [t_ for t_ in mono if -0.05 <= t_[0] <= full]
if len(table) < 4 and rax_g:
    # No linkage in the .geom: the axle rises about 2.4x the shock's stroke (a typical MX ratio).
    arm_len = (Vector(rax_g) - Vector(pivot_g)).length or 0.58
    hang = -math.asin(min(0.9, (rig.get("rearExtendedDrop") or 0.0) / 1000.0 / arm_len))
    table = [(s_ / 100.0, hang + math.asin(min(0.95, 2.4 * s_ / 100.0 / arm_len))) for s_ in range(0, 35)]
bike_root["linkage"] = json.dumps([[round(s_, 5), round(r_, 5)] for s_, r_ in table])
log("linkage", len(table), "points")

# ---------------------------------------------------------------- rider (body skinned from the game's own mesh)
RZ90 = Matrix.Rotation(math.radians(90), 4, 'Z')
if RD:
    BODY = os.path.join(RD, "body")
    rj = json.load(open(os.path.join(BODY, "rider.json")))
    RP = np.fromfile(os.path.join(BODY, "pos.f32"), np.float32).reshape(-1, 3)
    RN = np.fromfile(os.path.join(BODY, "nrm.f32"), np.float32).reshape(-1, 3)
    RU = np.fromfile(os.path.join(BODY, "uv.f32"), np.float32).reshape(-1, 2)
    RI = np.fromfile(os.path.join(BODY, "idx.u32"), np.uint32).reshape(-1, 3)
    SI = np.fromfile(os.path.join(BODY, "skin_idx.u32"), np.uint32).reshape(-1, 4)
    SW = np.fromfile(os.path.join(BODY, "skin_w.f32"), np.float32).reshape(-1, 4)
    rider_root = bpy.data.objects.new("CINEMA_rider_root", None); C_RIDER.objects.link(rider_root)
    rider_root.parent = E["chassis"]
    rider_root.matrix_basis = Matrix.Translation(g2b(geom.get("rider", (0.0, 0.0, 0.0))))
    arm_d = bpy.data.armatures.new("rider_rig"); arm = bpy.data.objects.new("rider_rig", arm_d)
    C_RIDER.objects.link(arm); arm.parent = rider_root
    bpy.context.view_layer.objects.active = arm
    bones = rj["bones"]
    children = {i: [] for i in range(len(bones))}
    for i, b in enumerate(bones):
        if b["parent"] is not None:
            children[b["parent"]].append(i)
    BMAT = []
    for b in bones:
        m = Matrix([b["bind"][0:4], b["bind"][4:8], b["bind"][8:12], b["bind"][12:16]])
        BMAT.append(M4 @ m @ M4.transposed())
    bpy.ops.object.mode_set(mode='EDIT')
    LONG = {"LeftKnee": 0.42, "RightKnee": 0.42, "LeftWrist": 0.09, "RightWrist": 0.09, "Head": 0.2}
    for i, b in enumerate(bones):
        eb = arm_d.edit_bones.new(b["name"])
        short = b["name"].replace("riderRIG_", "")
        h = BMAT[i].to_translation()
        ln = LONG.get(short)
        if ln is None:
            ch = [c for c in children[i] if "Twist" not in bones[c]["name"]] or children[i]
            ln = max(0.03, (BMAT[ch[0]].to_translation() - h).length) if ch else 0.06
        mm = BMAT[i].normalized() @ RZ90
        eb.head = h; eb.tail = h + Vector((0, 0.1, 0)); eb.matrix = mm; eb.length = ln
    for i, b in enumerate(bones):
        if b["parent"] is not None:
            arm_d.edit_bones[b["name"]].parent = arm_d.edit_bones[bones[b["parent"]]["name"]]
    bpy.ops.object.mode_set(mode='OBJECT')
    vb = np.stack([-RP[:, 0], -RP[:, 2], RP[:, 1]], -1)
    nb = np.stack([-RN[:, 0], -RN[:, 2], RN[:, 1]], -1)
    f = RI.astype(np.int32)
    me = bpy.data.meshes.new("rider_body")
    me.vertices.add(len(vb)); me.vertices.foreach_set("co", vb.ravel())
    me.loops.add(f.size); me.loops.foreach_set("vertex_index", f.ravel())
    me.polygons.add(len(f)); me.polygons.foreach_set("loop_start", np.arange(0, f.size, 3, dtype=np.int32))
    me.polygons.foreach_set("loop_total", np.full(len(f), 3, np.int32))
    uvl = me.uv_layers.new(name="UVMap"); uu = RU[f.ravel()].copy(); uu[:, 1] = 1 - uu[:, 1]; uvl.data.foreach_set("uv", uu.ravel())
    me.update(calc_edges=True); me.validate()
    me.polygons.foreach_set("use_smooth", np.ones(len(f), bool))
    me.normals_split_custom_set_from_vertices([tuple(x) for x in nb])
    body = bpy.data.objects.new("rider_body", me); C_RIDER.objects.link(body); body.parent = arm
    vgs = [body.vertex_groups.new(name=b["name"]) for b in bones]
    for k in range(4):
        for bi in np.unique(SI[:, k]):
            sel = np.nonzero((SI[:, k] == bi) & (SW[:, k] > 0))[0]
            if len(sel) and bi < len(vgs):
                w = SW[sel, k]
                for wv_ in np.unique(np.round(w, 3)):
                    ids = sel[np.round(w, 3) == wv_]
                    vgs[bi].add(ids.tolist(), float(wv_), 'ADD')
    mod = body.modifiers.new("rig", 'ARMATURE'); mod.object = arm
    suit = bpy.data.materials.new("rider_suit"); suit.use_nodes = True
    sb = suit.node_tree.nodes["Principled BSDF"]
    sb.inputs["Base Color"].default_value = (0.08, 0.08, 0.1, 1); sb.inputs["Roughness"].default_value = 0.7
    body.data.materials.append(suit)

    # static riding pose: IK to the grips and pegs (pose.py replaces it with the per-frame motion)
    def tgt(name, pos_game_chassis, parent=E["chassis"]):
        e = bpy.data.objects.new(name, None); C_RIDER.objects.link(e)
        e.empty_display_size = 0.04; e.parent = parent; e.location = g2b(pos_game_chassis)
        return e

    t_lh = bpy.data.objects.new("ik_hand_L", None); C_RIDER.objects.link(t_lh); t_lh.parent = E["steer"]; t_lh.location = g2b((-0.31, 0.24, -0.06))
    t_rh = bpy.data.objects.new("ik_hand_R", None); C_RIDER.objects.link(t_rh); t_rh.parent = E["steer"]; t_rh.location = g2b((0.31, 0.24, -0.06))
    t_lf = tgt("ik_foot_L", (-0.17, 0.40, -0.13))
    t_rf = tgt("ik_foot_R", (0.17, 0.40, -0.13))
    p_le = tgt("pole_elbow_L", (-0.9, 1.35, -0.05)); p_re = tgt("pole_elbow_R", (0.9, 1.35, -0.05))
    p_lk = tgt("pole_knee_L", (-0.35, 0.8, 1.2)); p_rk = tgt("pole_knee_R", (0.35, 0.8, 1.2))
    pb = arm.pose.bones
    for bn, t, pole in (("riderRIG_LeftElbow", t_lh, p_le), ("riderRIG_RightElbow", t_rh, p_re),
                        ("riderRIG_LeftKnee", t_lf, p_lk), ("riderRIG_RightKnee", t_rf, p_rk)):
        if bn in pb:
            c = pb[bn].constraints.new('IK'); c.target = t; c.chain_count = 2
            c.pole_target = pole; c.pole_angle = math.radians(-90)
    log("rider", rj.get("rider"), len(bones), "bones", len(vb), "verts")

# ---------------------------------------------------------------- lighting: the track's own sun and sky
amb = open(os.path.join(TD, "track.amb"), errors="ignore").read() if os.path.exists(os.path.join(TD, "track.amb")) else ""


def amb_block(txt, name):
    i = txt.find(name)
    while i >= 0:
        j = txt.find("{", i)
        if j >= 0 and txt[i + len(name):j].strip() == "":
            depth, k = 0, j
            while k < len(txt):
                depth += {"{": 1, "}": -1}.get(txt[k], 0)
                if depth == 0:
                    break
                k += 1
            return txt[j + 1:k]
        i = txt.find(name, i + 1)
    return ""


def amb_vals(blk):
    v = {}
    for line in blk.splitlines():
        if "=" in line and "{" not in line:
            k_, x_ = line.split("=", 1)
            try:
                v[k_.strip()] = float(x_.split(";")[0])
            except ValueError:
                v[k_.strip()] = x_.strip()
    return v


# the track's own light (lighting.py): sun direction/colour and ambient from the .amb, scaled like the game's clamp,
# a camera-side fill, and the track's own sky (a 2:1 panorama, or the upper-hemisphere dome padded to one)
from lighting import game_lighting, sky_panorama
world = bpy.data.worlds.new("CINEMA_sky"); sc.world = world; world.use_nodes = True
wn = world.node_tree.nodes; wl = world.node_tree.links
for x in list(wn):
    if x.type != 'OUTPUT_WORLD':
        wn.remove(x)
sky_cam = sky_gl = None
sky_info = tj.get("sky")
from lighting import amb_lighting
_sg, _a, _s = amb_lighting(amb)
sun_dir = g2b(_sg).normalized()
if sky_info:
    pano, u_sun, flip = sky_panorama(np.fromfile(os.path.join(TD, sky_info["file"]), np.uint8).reshape(sky_info["h"], sky_info["w"], 4))
    pano_png = os.path.join(OUT_TEX, "track_sky.png")
    ph, pw = pano.shape[:2]
    im = bpy.data.images.new("track_sky", pw, ph, alpha=True)
    im.pixels.foreach_set((pano[::-1].astype(np.float32) / 255.0).ravel())
    im.filepath_raw = pano_png; im.file_format = 'PNG'; im.save()
    sky = wn.new("ShaderNodeTexEnvironment"); sky.image = im
    wtc = wn.new("ShaderNodeTexCoord"); smp = wn.new("ShaderNodeMapping")
    if flip:
        smp.inputs["Scale"].default_value = (1, 1, -1)
    pano_sun_az = (u_sun - 0.5) * 2 * math.pi
    smp.inputs["Rotation"].default_value = (0, 0, pano_sun_az - math.atan2(sun_dir.y, -sun_dir.x))
    wl.new(wtc.outputs["Generated"], smp.inputs[0]); wl.new(smp.outputs[0], sky.inputs[0])
    bg_cam = wn.new("ShaderNodeBackground"); bg_cam.inputs["Strength"].default_value = 1.0
    wl.new(sky.outputs[0], bg_cam.inputs[0])
    sky_cam, sky_gl = bg_cam.outputs[0], sky.outputs[0]
    log("sky: the track's panorama", pw, "x", ph, "sun at u", round(u_sun, 3))
info = game_lighting(sc, amb, C_LIGHT, sky_camera=sky_cam, sky_glossy=sky_gl)
# A sheet without a reflection map (_r) mirrors no environment in the game, only the sun's highlight: tint
# its specular by the sun's colour so Cycles' sky reflection does not lay blue over it (the KTM orange read
# pink-red, B/R 0.16-0.20 in Cinema vs 0.07-0.15 in the F10 shots).
_sun_rgb = amb_lighting(amb)[2]
for _m in bpy.data.materials:
    if _m.get("cinema_no_refl") and _m.node_tree:
        for _b in _m.node_tree.nodes:
            if _b.type == 'BSDF_PRINCIPLED':
                _b.inputs["Specular Tint"].default_value = (*(c / max(_sun_rgb) for c in _sun_rgb), 1)
sun = bpy.data.objects["CINEMA_sun"]
log("light", {k: (tuple(round(x, 3) for x in v) if hasattr(v, "__len__") else round(v, 3)) for k, v in info.items()})

# ---------------------------------------------------------------- render settings
sc.render.engine = 'CYCLES'
cy = sc.cycles
cy.device = 'GPU'
cy.samples = 256; cy.use_denoising = True
cy.offscreen_dicing_scale = 16.0; cy.max_subdivisions = 7  # keep adaptive dicing bounded off-screen
cy.max_bounces = 12; cy.transparent_max_bounces = 64; cy.diffuse_bounces = 4; cy.glossy_bounces = 4
sc.render.resolution_x, sc.render.resolution_y = 1920, 1080
# The game draws with no tone curve: lighting.py scales sun + ambient so a flat lit white is 1.0, which is the
# game's white on screen. AgX compressed that to ~0.8 and crushed the mids (F10 007/008: rider and bike 10-20%
# darker than the game, blues lost); Standard shows it as the game does.
sc.view_settings.view_transform = 'Standard'
sc.view_settings.look = 'None'
sc.view_settings.exposure = 0.0
sc.render.film_transparent = False
log("scene built")

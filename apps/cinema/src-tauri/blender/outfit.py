"""MXB Cinema rider outfit: dress CINEMA_rider_root in the rider's own in-game gear.

Runs after animate.py + drive_scene.py, before pose.py (cinema_run.py). The app resolved the gear the way MXB App's
garage and Coach do (mxb_core::viewer::load_rider_model: the profile.ini loadout for the bike -> mods/rider helmets,
boots, protections as folders or .pkz, else the game's stock rider.pkz; suit and gloves .pnt under
riders/<rider>/paints|gloves) and wrote it to <rider_dir>/outfit: meshes plus every bound texture at full
resolution. <rider_dir>/edf holds each gear EDF's embedded _n bump maps, which the viewer drops. Material = the
game's: albedo from the paint, bump = *_n with normal = normalize((rgb - .5) * 2) and specular from its alpha.

Gear is bone-local in the game's bone frame (helmet -> riderRIG_Head, boots -> *KneeTwist, neckbrace -> Spine4,
armour -> riderRIG_armour), so pose.py's boot skinning and IK keep working.
"""
import bpy, sys, os, json, base64, argparse, math
import numpy as np
from mathutils import Matrix
from cinema_job import JOB, OUT_TEX

RD = JOB['rider_dir']
OUT = os.path.join(RD, "outfit")
EDFD = os.path.join(RD, "edf")

RZ90 = Matrix.Rotation(math.radians(90), 4, 'Z')
LINK = {"helmet": "riderRIG_Head", "boot_l": "riderRIG_LeftKneeTwist", "boot_r": "riderRIG_RightKneeTwist",
        "armour": "riderRIG_armour", "neckbrace": "riderRIG_Spine4"}


def log(*a):
    print("OUTFIT", *a, flush=True)


J = json.load(open(os.path.join(OUT, "outfit.json")))
EIDX = json.load(open(os.path.join(EDFD, "index.json"))) if os.path.exists(os.path.join(EDFD, "index.json")) else []
lo = J["loadout"]
log("loadout:", {k: lo.get(k) for k in ("rider", "helmet", "helmetPaint", "gogglesPaint", "suitPaint",
                                         "glovesPaint", "boots", "bootsPaint", "protection")})

# ---------------------------------------------------------------- 2. textures (full res, cached as PNG beside the raw sheet)
_imgs = {}


def image(key, raw, w, h, non_color=False):
    if key in _imgs:
        return _imgs[key]
    png = raw + ".png"
    if not os.path.exists(png):
        a = np.fromfile(raw, np.uint8).reshape(h, w, 4)[::-1].astype(np.float32) / 255.0
        im = bpy.data.images.new("tmp_" + key, w, h, alpha=True)
        im.pixels.foreach_set(a.ravel())
        im.filepath_raw = png
        im.file_format = 'PNG'
        im.save()
        bpy.data.images.remove(im)
    safe = "".join(c if c.isalnum() or c in "-_." else "_" for c in key)
    dst = os.path.join(OUT_TEX, "rider_" + safe + ".png")
    if not os.path.exists(dst):
        import shutil
        shutil.copyfile(png, dst)
    im = bpy.data.images.load(dst, check_existing=True)
    im.name = "cin_" + key
    im.colorspace_settings.name = 'Non-Color' if non_color else 'sRGB'
    im.alpha_mode = 'CHANNEL_PACKED'
    _imgs[key] = im
    return im


def colour_tex(part, name):
    for t in part["textures"]:
        if t["name"].lower() == name.lower():
            return image(f"{part['part']}_{t['name']}_{t['w']}x{t['h']}", os.path.join(OUT, t["file"]), t["w"], t["h"])
    return None


def bump_tex(part, name):
    """The paint's own _n first (a suit .pnt ships one), else the EDF's embedded one."""
    want = (name + "_n").lower()
    for t in part["textures"]:
        if t["name"].lower() == want:
            return image(f"{part['part']}_{t['name']}_{t['w']}x{t['h']}", os.path.join(OUT, t["file"]), t["w"], t["h"], True)
    for e in EIDX:
        if e["name"].lower() == want:
            return image(f"edf_{e['file'][:-5]}", os.path.join(EDFD, e["file"]), e["w"], e["h"], True)
    return None


def refl_tex(part, name):
    """The `_r` reflection map: the paint's own first, else the EDF's (the app writes both _n and _r there)."""
    want = (name + "_r").lower()
    for t in part["textures"]:
        if t["name"].lower() == want:
            return image(f"{part['part']}_{t['name']}_{t['w']}x{t['h']}", os.path.join(OUT, t["file"]), t["w"], t["h"], True)
    for e in EIDX:
        if e["name"].lower() == want:
            return image(f"edf_{e['file'][:-5]}", os.path.join(EDFD, e["file"]), e["w"], e["h"], True)
    return None


# ---------------------------------------------------------------- 3. the game's material model
def game_material(name, albedo, bump, kind, refl=None):
    m = bpy.data.materials.new("cin_" + name)
    m.use_nodes = True
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    b = N["Principled BSDF"]
    if albedo is not None:
        t = N.new("ShaderNodeTexImage"); t.image = albedo; t.interpolation = 'Cubic'
        L.new(t.outputs["Color"], b.inputs["Base Color"])
    if bump is not None:
        nb = N.new("ShaderNodeTexImage"); nb.image = bump; nb.interpolation = 'Cubic'
        # normal = normalize((Bump.xyz - 0.5) * 2): exactly what the tangent-space Normal Map node decodes
        nm = N.new("ShaderNodeNormalMap"); nm.space = 'TANGENT'; nm.uv_map = "UVMap"
        L.new(nb.outputs["Color"], nm.inputs["Color"])
        L.new(nm.outputs["Normal"], b.inputs["Normal"])
        # specular from Bump.a: game spec intensity -> IOR level, and shinier where it is high
        L.new(nb.outputs["Alpha"], b.inputs["Specular IOR Level"])
        inv = N.new("ShaderNodeMapRange")
        inv.inputs["To Min"].default_value = {"shell": 0.32, "boot": 0.55, "suit": 0.85, "glove": 0.5}.get(kind, 0.6)
        inv.inputs["To Max"].default_value = {"shell": 0.12, "boot": 0.28, "suit": 0.5, "glove": 0.26}.get(kind, 0.3)
        L.new(nb.outputs["Alpha"], inv.inputs["Value"])
        L.new(inv.outputs["Result"], b.inputs["Roughness"])
    else:
        b.inputs["Roughness"].default_value = {"shell": 0.2, "suit": 0.75, "armour": 0.45}.get(kind, 0.5)
    if refl is not None and kind != "lens":
        # MXB's reflection map (_r, EDF `reflection` min/max): how much of the environment the game mirrors on
        # top of the diffuse. It is not metalness - the albedo stays the colour (a chain is a dark sheet with
        # bright glints, not polished steel) - so it pulls the roughness toward a mirror where it is high.
        rt = N.new("ShaderNodeTexImage"); rt.image = refl; rt.interpolation = 'Cubic'
        sharp = N.new("ShaderNodeMath"); sharp.operation = 'MULTIPLY_ADD'
        sharp.inputs[1].default_value = -0.8; sharp.inputs[2].default_value = 1.0      # 1 - 0.8 * r
        L.new(rt.outputs["Color"], sharp.inputs[0])
        rough = N.new("ShaderNodeMath"); rough.operation = 'MULTIPLY'
        src = [l.from_socket for l in L if l.to_socket == b.inputs["Roughness"]]
        if src:
            L.new(src[0], rough.inputs[0])
        else:
            rough.inputs[0].default_value = b.inputs["Roughness"].default_value
        L.new(sharp.outputs[0], rough.inputs[1])
        L.new(rough.outputs[0], b.inputs["Roughness"])
        if kind == "shell":
            # the clear coat is the same reflection: a shell whose _r is near black (Airoh's: 2%) is the matte
            # black the game draws, not a lacquer mirroring the sky blue
            cw = N.new("ShaderNodeMath"); cw.operation = 'MULTIPLY'; cw.inputs[1].default_value = 0.8
            L.new(rt.outputs["Color"], cw.inputs[0]); L.new(cw.outputs[0], b.inputs["Coat Weight"])
    if kind == "shell":       # painted + clear-coated helmet shell (coat weight from _r when the EDF has one)
        b.inputs["Coat Weight"].default_value = 0.8
        b.inputs["Coat Roughness"].default_value = 0.04
    if kind == "suit":        # textile: soft sheen, no hard highlight
        b.inputs["Sheen Weight"].default_value = 0.35
        b.inputs["Sheen Roughness"].default_value = 0.45
    if kind == "lens":
        # The game's lens (helmet.edf `lens`: grey streak sheet, _r 255, reflection min 0.1 / max 10, specular 12) is
        # a mirror: what the F10 shots show in it is the track reflected - sand-gold streaks, the blue and red of the
        # banners at its edge - smeared into vertical streaks across the curved lens. So: a metal over the sheet at
        # full detail (the sheet is the reflectance, 0.3-0.9), smooth across and stretched along the lens's height
        # (anisotropic, tangent around the head's vertical axis turned 90 degrees). It mirrors the track, not the
        # player's own bike and rider: the game's reflections come from an environment capture without them, and
        # Cycles showed the orange fender in the lens (see the glossy-visibility rule at the end of this file).
        b.inputs["Metallic"].default_value = 1.0
        b.inputs["Roughness"].default_value = 0.14
        b.inputs["Anisotropic"].default_value = 0.9
        b.inputs["Anisotropic Rotation"].default_value = 0.25
        tg = N.new("ShaderNodeTangent"); tg.direction_type = 'RADIAL'; tg.axis = 'Z'
        L.new(tg.outputs["Tangent"], b.inputs["Tangent"])
        # Reflectance = the sheet in the game's iridium gold. The F10 lens (007: mean sRGB 103/84/61) over the
        # untinted mirror's (96/83/68 in v5) asks this per channel; red runs a little over 1 at the sheet's peak.
        gold = N.new("ShaderNodeMix"); gold.data_type = 'RGBA'; gold.blend_type = 'MULTIPLY'
        gold.inputs["Factor"].default_value = 1.0
        gold.inputs["B"].default_value = (1.2, 0.95, 0.6, 1)
        src = [l.from_socket for l in L if l.to_socket == b.inputs["Base Color"]]
        if src:
            L.new(src[0], gold.inputs["A"])
        else:
            gold.inputs["A"].default_value = (0.54, 0.54, 0.54, 1)
        L.new(gold.outputs["Result"], b.inputs["Base Color"])
    if kind == "skin":
        b.inputs["Base Color"].default_value = (0.15, 0.075, 0.048, 1)  # the game's neck tone (F10)
        b.inputs["Specular IOR Level"].default_value = 0.25
        b.inputs["Roughness"].default_value = 0.6
    if kind == "hide":
        m.node_tree.nodes.remove(b)
        tr = N.new("ShaderNodeBsdfTransparent")
        L.new(tr.outputs[0], N["Material Output"].inputs["Surface"])
    m["_cinema"] = 1
    return m


def kind_of(part, tex):
    t = (tex or "").lower()
    if part == "helmet":
        return "lens" if "lens" in t else ("goggle" if "goggle" in t else "shell")
    if part == "body" and "glove" in t:
        return "glove"        # gloves are the body mesh's hand submesh, painted by the gloves sheet
    return {"boots": "boot", "protection": "armour"}.get(part, "suit")


_mats = {}


def material_for(part, tex):
    key = (part["part"], tex)
    if key in _mats:
        return _mats[key]
    if tex in (None, "hide"):
        m = game_material(f"{part['part']}_hide", None, None, "hide")
    elif tex == "face":
        m = game_material("face", None, None, "skin")
    else:
        k = kind_of(part["part"], tex)
        m = game_material(f"{part['part']}_{tex}", colour_tex(part, tex), bump_tex(part, tex), k, refl_tex(part, tex))
    _mats[key] = m
    return m


# ---------------------------------------------------------------- 4. meshes
def arr(s, dt):
    return np.frombuffer(base64.b64decode(s), dt)


def to_local(P):
    """Coach/EDF node space -> the bone-local Blender frame build_scene.py's gear uses: (x, -z, y)."""
    return np.stack([P[:, 0], -P[:, 2], P[:, 1]], 1)


def build_mesh(part, node, name):
    P = to_local(arr(node["positions"], '<f4').reshape(-1, 3))
    Nn = to_local(arr(node["normals"], '<f4').reshape(-1, 3))
    UV = arr(node["uvs"], '<f4').reshape(-1, 2)
    I = arr(node["indices"], '<u4').reshape(-1, 3)
    me = bpy.data.meshes.new(name)
    me.from_pydata(P.tolist(), [], I.tolist())
    uvl = me.uv_layers.new(name="UVMap")
    li = I.ravel()
    uv = UV[li].copy()
    uv[:, 1] = 1.0 - uv[:, 1]              # EDF v runs top-down
    uvl.data.foreach_set("uv", uv.ravel())
    fill_materials(me, part, node)
    me.polygons.foreach_set("use_smooth", [True] * len(me.polygons))
    me.normals_split_custom_set_from_vertices(Nn.tolist())
    return me


def fill_materials(me, part, node):
    me.materials.clear()
    subs = node["submeshes"] or [{"texture": node.get("texture"), "triStart": 0, "triCount": len(me.polygons)}]
    idx = np.zeros(len(me.polygons), np.int32)
    slots = []
    for s in subs:
        m = material_for(part, s["texture"])
        if m.name not in [x.name for x in slots]:
            slots.append(m); me.materials.append(m)
        idx[s["triStart"]:s["triStart"] + s["triCount"]] = [x.name for x in slots].index(m.name)
    me.polygons.foreach_set("material_index", idx)


rider = bpy.data.collections["rider"]
arm = bpy.data.objects["rider_rig"]
O = bpy.data.objects
bpy.context.view_layer.update()
parts = {p["part"]: p for p in J["parts"]}

# The game draws no protection when the loadout leaves the slot empty (Sean's F10 shots: bare neck, no
# chest piece), where Coach previews the stock `full` set. Follow the game: drop the stock pieces.
if not J["loadout"].get("protection"):
    parts.pop("protection", None)
    for nm in ("neckbrace", "armour"):
        o = O.get(nm)
        if o is not None:
            log("protection: none in the loadout, removing stock", nm)
            bpy.data.objects.remove(o)

for pname in ("helmet", "boots", "protection"):
    p = parts.get(pname)
    if p is None:
        log("WARN no", pname, "resolved")
        continue
    for node in p["nodes"]:
        nm = node["name"]
        me = build_mesh(p, node, "cin_" + nm)
        o = O.get(nm)
        if o is not None and o.type == 'MESH':       # swap the stock gear's mesh, keep its bone placement
            old = o.data
            o.data = me
            if old.users == 0:
                bpy.data.meshes.remove(old)
        else:
            o = bpy.data.objects.new(nm, me)
            rider.objects.link(o)
            bn = LINK.get(nm)
            if bn is None or bn not in arm.pose.bones:
                # riderRIG_armour is a dummy in rider.edf, not a skin bone. Every protection piece is
                # authored around one shared mount (Coach's protAnchor), and the neck brace's mount is
                # Spine4 (gfx.cfg neckbracelinkobj), so the chest piece rides Spine4 in the same frame.
                bn = "riderRIG_Spine4"
            want = arm.matrix_world @ arm.pose.bones[bn].matrix @ RZ90.inverted()
            o.parent = arm; o.parent_type = 'BONE'; o.parent_bone = bn
            bpy.context.view_layer.update()
            o.matrix_world = want
        o.name = nm
        log(pname, nm, len(me.vertices), "verts", [m.name for m in me.materials])

# body: same vertices/skin as the scene's rider_body; re-dress with the suit + gloves paint
body = O.get("rider_body")
bp = parts.get("body")
if body is not None and bp is not None:
    node = bp["nodes"][0]
    I = arr(node["indices"], '<u4').reshape(-1, 3)
    me = body.data
    if len(me.polygons) == len(I):
        UV = arr(node["uvs"], '<f4').reshape(-1, 2)
        loops = np.zeros(len(me.loops), np.int32)
        me.loops.foreach_get("vertex_index", loops)
        uv = UV[loops].copy(); uv[:, 1] = 1.0 - uv[:, 1]
        uvl = me.uv_layers.active or me.uv_layers.new(name="UVMap")
        uvl.name = "UVMap"
        uvl.data.foreach_set("uv", uv.ravel())
        fill_materials(me, bp, node)
        log("body", len(me.polygons), "tris", [m.name for m in me.materials])
    else:
        log("WARN body topology differs", len(me.polygons), len(I))

log("dressed", lo.get("rider") or "default_mx")

# The game's reflections (lens, chrome, coats) are a capture of the track around the rider, which holds neither the
# player's bike nor the player: keep both out of Cycles' glossy rays so no part mirrors them (the goggle lens showed
# the orange front fender; the game's shows the track).
for _c in ("bike", "rider"):
    _coll = bpy.data.collections.get(_c)
    if _coll is not None:
        for _o in _coll.all_objects:
            _o.visible_glossy = False
log("glossy rays skip the bike and rider (the game's reflection capture holds the track only)")

"""MXB Cinema "realism" options for the cinematic preset: the game's low-poly, clean-sheet look taken toward a photo.
Applied after cinematic.apply() on a built, posed scene (one frame):

    import cinematic, realism
    cinematic.apply(sc, cam, frame, ...)
    realism.apply(sc, frame)
    ... render (realism.enable_passes(sc) first for the AI-pass EXR) ...
    cinematic.finish(png); realism.lens(png)
    realism.ai_passes(sc, outdir, stem)          # depth / normal / object-ID / edge images for an AI photo pass

  geometry  Catmull-Clark at render time on the helmet, the rider body (suit + gloves), boots and the bike's
            chassis / steer / fork meshes; edges sharper than 40 deg are creased and marked sharp first, so panel
            edges stay hard and only the faceted curves round off. UVs keep their corners (decals don't swim).
  plastics  the bike sheet's plastics (dgsbike / KTMMAP): clear coat with a faint orange-peel bump, roughness broken
            up by a grunge noise, light scuffs and micro-scratches where the surface turns sharply (Pointiness)
  mud       the Bren-style mud layer (scratch/dirt-test/bren/dgsbike_mud2.tga: alpha = mud, colour 152/118/88) over
            the bike sheet, matte and slightly raised; light dust on boots, lower suit and gloves from a height +
            cavity (AO) mask, procedural
  fabric    sheen on the suit and gloves, softer roughness
  rubber    tyres matte with roughness variation
  light     a real outdoor HDRI (cinema/hdri: Poly Haven's ninomaru_teien 4K, CC0) for light and
            reflections, turned to the track's sun, its sun disc clipped (the lamp is the sun), scaled to the matched
            fill; the camera still sees the track's own sky picture
  rider     suit folds gathered at the bent joints, glove grain, helmet clear coat with micro-scratches and less
            glossy decals; the visor mirrors the HDRI (the bike and rider stay out of glossy rays)
  stanchion the fork tubes' UV strip on the bike sheet: no mud, part metal (0.7), lightly brushed, clear-coated, own decals
  haze      (on the PNG, from the depth pass) a light aerial haze on the far trees
  lens      (on the PNG) slight barrel distortion and lateral chromatic aberration toward the edges
"""
import math
import os
import numpy as np
import bpy
from mathutils import Vector

MUD_TGA = r"C:\dev\MXB\claude-work\scratch\dirt-test\bren\dgsbike_mud2.tga"
MUD_RGB = (152, 118, 88)
MUD_AMOUNT = 1.0          # the app's Mud slider (0..1): 0 a clean bike and clean gear; 1 about 2.5x the prototype's layer, wider spread, double the dust
PLASTIC_SHEETS = ("dgsbike", "ktmmap")
SUBDIV = {"helmet": 2, "rider_body": 1, "boot_l": 1, "boot_r": 1}
BIKE_SUBDIV_PARTS = ()   # v1 subdivided chassis/steer/fork too: their split, creased panels tore at the shroud edges
# the fork stanchions on the bike sheet (steer mesh, ray-cast at the tight 007 frame): one UV strip, u 0.0035-0.0346,
# v 0.436-0.655, and its neighbour strip to u ~0.05. No mud there; brushed metal instead.
STANCHION_UV = (0.0, 0.43, 0.052, 0.66)
HDRI = r"C:\dev\MXB\claude-work\cinema\hdri\ninomaru_teien_4k.exr"   # Poly Haven, CC0, 4K
STANCHION_METALLIC = 0.7
COAT_PLASTIC = 0.4      # 0.7 laid the sky over the shaded inner shroud as a blue-violet patch (v1)
MAX_FACES = 400_000
SHARP_DEG = 40.0


def log(*a):
    print("REALISM", *a, flush=True)


def _lin(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


# ---------------------------------------------------------------- geometry
def _mark_sharp(me):
    import bmesh
    bm = bmesh.new(); bm.from_mesh(me); bm.edges.ensure_lookup_table()
    lim = math.radians(SHARP_DEG)
    flags = np.zeros(len(bm.edges), bool)
    for e in bm.edges:
        if len(e.link_faces) != 2:
            flags[e.index] = True
        else:
            try:
                flags[e.index] = e.calc_face_angle() > lim
            except ValueError:
                flags[e.index] = True
    bm.free()
    for nm, typ in (("sharp_edge", 'BOOLEAN'), ("crease_edge", 'FLOAT')):
        a = me.attributes.get(nm) or me.attributes.new(nm, typ, 'EDGE')
        a.data.foreach_set("value", flags if typ == 'BOOLEAN' else flags.astype(np.float32))
    return int(flags.sum())


def _subdivide(o, levels):
    me = o.data
    if len(me.polygons) > MAX_FACES:
        log("subdiv skip", o.name, len(me.polygons), "faces")
        return
    n = _mark_sharp(me)
    me.polygons.foreach_set("use_smooth", [True] * len(me.polygons))
    m = o.modifiers.new("REALISM_subsurf", 'SUBSURF')
    m.levels = 0; m.render_levels = levels; m.quality = 3; m.use_creases = True; m.use_limit_surface = True
    m.uv_smooth = 'PRESERVE_CORNERS'; m.boundary_smooth = 'PRESERVE_CORNERS'
    log("subdiv", o.name, "x%d" % levels, len(me.polygons), "faces,", n, "sharp edges")


def _geometry():
    O = bpy.data.objects
    for nm, lv in SUBDIV.items():
        if nm in O and O[nm].type == 'MESH':
            _subdivide(O[nm], lv)
    coll = bpy.data.collections.get("bike")
    for o in (coll.all_objects if coll else []):
        if o.type == 'MESH' and not o.hide_render and o.name.split('.')[0] in BIKE_SUBDIV_PARTS:
            _subdivide(o, 1)


# ---------------------------------------------------------------- shader helpers
def _principled(m):
    return next((n for n in m.node_tree.nodes if n.type == 'BSDF_PRINCIPLED'), None) if m.node_tree else None


def _src(L, sock):
    ls = [l for l in L if l.to_socket == sock]
    return ls[0].from_socket if ls else None


def _value_of(N, L, sock):
    """The socket feeding `sock`, or a Value node holding its current default."""
    s = _src(L, sock)
    if s is not None:
        return s
    v = N.new("ShaderNodeValue"); v.outputs[0].default_value = sock.default_value
    return v.outputs[0]


def _math(N, L, op, a, b, clamp=False):
    n = N.new("ShaderNodeMath"); n.operation = op; n.use_clamp = clamp
    for i, x in enumerate((a, b)):
        if isinstance(x, (int, float)):
            n.inputs[i].default_value = x
        else:
            L.new(x, n.inputs[i])
    return n.outputs[0]


def _mix_col(N, L, a, b, fac):
    n = N.new("ShaderNodeMix"); n.data_type = 'RGBA'
    for sock, x in ((n.inputs["A"], a), (n.inputs["B"], b)):
        if isinstance(x, tuple):
            sock.default_value = x
        else:
            L.new(x, sock)
    if isinstance(fac, float):
        n.inputs["Factor"].default_value = fac
    else:
        L.new(fac, n.inputs["Factor"])
    return n.outputs["Result"]


def _noise(N, L, scale, detail=6.0, rough=0.6, coord=None):
    t = N.new("ShaderNodeTexNoise"); t.inputs["Scale"].default_value = scale
    t.inputs["Detail"].default_value = detail; t.inputs["Roughness"].default_value = rough
    if coord is not None:
        L.new(coord, t.inputs["Vector"])
    return t.outputs["Fac"]


def _ramp(N, L, x, lo, hi):
    """smooth 0..1 step of x between lo and hi"""
    n = N.new("ShaderNodeMapRange"); n.interpolation_type = 'SMOOTHSTEP'
    n.inputs["From Min"].default_value = lo; n.inputs["From Max"].default_value = hi
    L.new(x, n.inputs["Value"])
    return n.outputs["Result"]


# ---------------------------------------------------------------- plastics + mud
def _plastic(m, mud_img):
    N, L = m.node_tree.nodes, m.node_tree.links
    b = _principled(m)
    obj = N.new("ShaderNodeTexCoord").outputs["Object"]
    # roughness: the sheet's, broken up by a grunge noise
    rough = _value_of(N, L, b.inputs["Roughness"])
    grunge = _noise(N, L, 18.0, 10.0, 0.65, obj)
    rough = _math(N, L, 'MULTIPLY', rough, _math(N, L, 'ADD', _math(N, L, 'MULTIPLY', grunge, 0.7), 0.65))   # x 0.65-1.35
    # scuffs and micro-scratches where the surface turns (Pointiness > ~0.53) and in a sparse noise
    geo = N.new("ShaderNodeNewGeometry")
    edge = _ramp(N, L, geo.outputs["Pointiness"], 0.52, 0.6)
    scratch = _ramp(N, L, _noise(N, L, 240.0, 2.0, 0.5, obj), 0.62, 0.7)
    scuff = _math(N, L, 'MULTIPLY', _math(N, L, 'MAXIMUM', edge, _math(N, L, 'MULTIPLY', scratch, 0.5)),
                  _ramp(N, L, _noise(N, L, 6.0, 4.0, 0.5, obj), 0.45, 0.65), clamp=True)
    scuff = _math(N, L, 'MULTIPLY', scuff, 0.6)
    base = _src(L, b.inputs["Base Color"]) or b.inputs["Base Color"].default_value[:]
    base = _mix_col(N, L, base, (0.42, 0.40, 0.38, 1), _math(N, L, 'MULTIPLY', scuff, 0.35))   # scuffed = paler, greyer
    rough = _math(N, L, 'ADD', rough, _math(N, L, 'MULTIPLY', scuff, 0.35))
    coat_w = _math(N, L, 'SUBTRACT', COAT_PLASTIC, _math(N, L, 'MULTIPLY', scuff, 0.3), clamp=True)
    # orange peel on the coat
    peel = N.new("ShaderNodeBump"); peel.inputs["Strength"].default_value = 0.035; peel.inputs["Distance"].default_value = 0.0005
    L.new(_noise(N, L, 900.0, 2.0, 0.5, obj), peel.inputs["Height"])
    # mud over all of it (bike sheet only: the mud image is painted on the dgsbike UV)
    if mud_img is not None and "dgsbike" in m.name.lower():
        uv = N.new("ShaderNodeUVMap"); uv.uv_map = "UVMap"
        t = N.new("ShaderNodeTexImage"); t.image = mud_img; t.interpolation = 'Cubic'
        L.new(uv.outputs["UV"], t.inputs["Vector"])
        # the layer is pixel noise over the whole sheet: on its own it reads as square specks up close (v1); taken
        # as where mud may be, and shaped by a fine procedural splatter, it reads as dried spatter and smears
        splat = _ramp(N, L, _noise(N, L, 160.0, 8.0, 0.7, obj), 0.48 - 0.14 * MUD_AMOUNT, 0.62)
        clumps = _ramp(N, L, _noise(N, L, 9.0, 4.0, 0.5, obj), 0.35, 0.75)
        mud = _math(N, L, 'MULTIPLY', _ramp(N, L, t.outputs["Alpha"], 0.03, 0.35),
                    _math(N, L, 'MULTIPLY', splat, _math(N, L, 'ADD', _math(N, L, 'MULTIPLY', clumps, 0.6), 0.4)), clamp=True)
        # not on the fork stanchions
        sepuv = N.new("ShaderNodeSeparateXYZ"); L.new(uv.outputs["UV"], sepuv.inputs[0])
        u0, v0, u1, v1 = STANCHION_UV
        inside = _math(N, L, 'MULTIPLY',
                       _math(N, L, 'MULTIPLY', _math(N, L, 'GREATER_THAN', sepuv.outputs["X"], u0), _math(N, L, 'LESS_THAN', sepuv.outputs["X"], u1)),
                       _math(N, L, 'MULTIPLY', _math(N, L, 'GREATER_THAN', sepuv.outputs["Y"], v0), _math(N, L, 'LESS_THAN', sepuv.outputs["Y"], v1)))
        mud = _math(N, L, 'MULTIPLY', _math(N, L, 'MULTIPLY', mud, _math(N, L, 'SUBTRACT', 1.0, inside, clamp=True)), 2.5 * MUD_AMOUNT, clamp=True)
        mc = tuple(_lin(c) for c in MUD_RGB) + (1,)
        mud_col = _mix_col(N, L, mc, (mc[0] * 0.55, mc[1] * 0.55, mc[2] * 0.55, 1), _noise(N, L, 120.0, 4.0, 0.6, obj))
        base = _mix_col(N, L, base, mud_col, mud)
        rough = _mix_col(N, L, rough, (0.93, 0.93, 0.93, 1), mud)
        coat_w = _math(N, L, 'MULTIPLY', coat_w, _math(N, L, 'SUBTRACT', 1.0, mud, clamp=True))
        mb = N.new("ShaderNodeBump"); mb.inputs["Strength"].default_value = 0.35; mb.inputs["Distance"].default_value = 0.002
        L.new(mud, mb.inputs["Height"])
        nrm = _src(L, b.inputs["Normal"])
        if nrm is not None:
            L.new(nrm, mb.inputs["Normal"])
        L.new(mb.outputs["Normal"], b.inputs["Normal"])
    if mud_img is not None and "dgsbike" in m.name.lower():
        # stanchions: the sheet's own silver and decals (the yellow ERIC AUGE), part metal, lightly brushed, under a
        # clear coat. Full metal with no coat mirrored the dark trees and read near black (v2/v3; the game and the
        # matched renders show light silver)
        tg = N.new("ShaderNodeTangent"); tg.direction_type = 'UV_MAP'; tg.uv_map = "UVMap"
        L.new(tg.outputs["Tangent"], b.inputs["Tangent"])
        L.new(_math(N, L, 'MULTIPLY', inside, STANCHION_METALLIC), b.inputs["Metallic"])
        L.new(_math(N, L, 'MULTIPLY', inside, 0.3), b.inputs["Anisotropic"])
        b.inputs["Anisotropic Rotation"].default_value = 0.25
        rough = _mix_col(N, L, rough, (0.25, 0.25, 0.25, 1), inside)
        coat_w = _math(N, L, 'ADD', _math(N, L, 'MULTIPLY', coat_w, _math(N, L, 'SUBTRACT', 1.0, inside, clamp=True)),
                       _math(N, L, 'MULTIPLY', inside, 0.4))
    L.new(base, b.inputs["Base Color"])
    if rough.type == 'RGBA':                         # the mud mix is a colour mix: back to one value
        rr = N.new("ShaderNodeRGBToBW"); L.new(rough, rr.inputs[0]); rough = rr.outputs[0]
    L.new(rough, b.inputs["Roughness"])
    L.new(coat_w, b.inputs["Coat Weight"])
    b.inputs["Coat Roughness"].default_value = 0.05
    L.new(peel.outputs["Normal"], b.inputs["Coat Normal"])
    log("plastic", m.name, "mud" if (mud_img is not None and "dgsbike" in m.name.lower()) else "")


# ---------------------------------------------------------------- rider dust + fabric
def _dust(m, z_low, amount):
    amount *= 2.0 * MUD_AMOUNT
    N, L = m.node_tree.nodes, m.node_tree.links
    b = _principled(m)
    geo = N.new("ShaderNodeNewGeometry")
    sep = N.new("ShaderNodeSeparateXYZ"); L.new(geo.outputs["Position"], sep.inputs[0])
    low = N.new("ShaderNodeMapRange"); low.interpolation_type = 'SMOOTHSTEP'
    low.inputs["From Min"].default_value = z_low + 0.55; low.inputs["From Max"].default_value = z_low - 0.1
    L.new(sep.outputs["Z"], low.inputs["Value"])
    ao = N.new("ShaderNodeAmbientOcclusion"); ao.inputs["Distance"].default_value = 0.05; ao.samples = 8
    cav = _math(N, L, 'SUBTRACT', 1.0, ao.outputs["AO"], clamp=True)
    obj = N.new("ShaderNodeTexCoord").outputs["Object"]
    brk = _ramp(N, L, _noise(N, L, 14.0, 8.0, 0.6, obj), 0.35, 0.7)
    mask = _math(N, L, 'MULTIPLY', _math(N, L, 'ADD', _math(N, L, 'MULTIPLY', low.outputs["Result"], 0.8), _math(N, L, 'MULTIPLY', cav, 0.6)),
                 _math(N, L, 'MULTIPLY', brk, amount), clamp=True)
    base = _src(L, b.inputs["Base Color"]) or b.inputs["Base Color"].default_value[:]
    dust = (_lin(150), _lin(124), _lin(98), 1)
    L.new(_mix_col(N, L, base, dust, mask), b.inputs["Base Color"])
    rough = _value_of(N, L, b.inputs["Roughness"])
    L.new(_math(N, L, 'ADD', rough, _math(N, L, 'MULTIPLY', mask, 0.5), clamp=True), b.inputs["Roughness"])


def _bump_on(N, L, b, height, strength, distance):
    bp = N.new("ShaderNodeBump"); bp.inputs["Strength"].default_value = strength; bp.inputs["Distance"].default_value = distance
    L.new(height, bp.inputs["Height"])
    nrm = _src(L, b.inputs["Normal"])
    if nrm is not None:
        L.new(nrm, bp.inputs["Normal"])
    L.new(bp.outputs["Normal"], b.inputs["Normal"])


def _fabric(m, joints=(), wrinkles=0.0, grain=0.0):
    """Sheen and softer roughness; `wrinkles`: folds (distorted wave bands broken by noise) gathered at the bent
    joints (world points at the frame: elbows, knees, hips, waist, shoulders), a little everywhere else; `grain`: a
    fine leather/fabric bump (gloves)."""
    b = _principled(m)
    b.inputs["Sheen Weight"].default_value = 0.6; b.inputs["Sheen Roughness"].default_value = 0.35
    N, L = m.node_tree.nodes, m.node_tree.links
    rough = _value_of(N, L, b.inputs["Roughness"])
    L.new(_math(N, L, 'ADD', rough, 0.08, clamp=True), b.inputs["Roughness"])
    obj = N.new("ShaderNodeTexCoord").outputs["Object"]
    if wrinkles > 0:
        geo = N.new("ShaderNodeNewGeometry")
        near = None
        for p in joints:
            d = N.new("ShaderNodeVectorMath"); d.operation = 'DISTANCE'; L.new(geo.outputs["Position"], d.inputs[0])
            d.inputs[1].default_value = tuple(p)
            w = _ramp(N, L, d.outputs["Value"], 0.24, 0.07)
            near = w if near is None else _math(N, L, 'MAXIMUM', near, w)
        wave = N.new("ShaderNodeTexWave"); wave.wave_type = 'BANDS'; wave.bands_direction = 'DIAGONAL'
        wave.inputs["Scale"].default_value = 9.0; wave.inputs["Distortion"].default_value = 9.0
        wave.inputs["Detail"].default_value = 3.0; wave.inputs["Detail Scale"].default_value = 1.5
        L.new(obj, wave.inputs["Vector"])
        folds = _math(N, L, 'MULTIPLY', wave.outputs["Fac"], _ramp(N, L, _noise(N, L, 5.0, 3.0, 0.5, obj), 0.3, 0.7))
        amt = _math(N, L, 'ADD', 0.15, _math(N, L, 'MULTIPLY', near, 0.85)) if near is not None else 0.4
        _bump_on(N, L, b, _math(N, L, 'MULTIPLY', folds, amt), wrinkles, 0.02)
    if grain > 0:
        _bump_on(N, L, b, _noise(N, L, 420.0, 6.0, 0.6, obj), grain, 0.0008)


def _helmet_shell(m):
    """Clear-coat gloss with fine micro-scratches in the coat; decals (the bright parts of the sheet) a touch less
    glossy than the paint."""
    N, L = m.node_tree.nodes, m.node_tree.links
    b = _principled(m)
    for l in [l for l in L if l.to_socket == b.inputs["Coat Weight"]]:
        L.remove(l)
    b.inputs["Coat Weight"].default_value = 0.5; b.inputs["Coat Roughness"].default_value = 0.09   # between v1's gloss and v2's satin (Sean, v3)
    obj = N.new("ShaderNodeTexCoord").outputs["Object"]
    sc_ = N.new("ShaderNodeBump"); sc_.inputs["Strength"].default_value = 0.08; sc_.inputs["Distance"].default_value = 0.0003
    scr = N.new("ShaderNodeTexWave"); scr.wave_type = 'RINGS'; scr.inputs["Scale"].default_value = 60.0
    scr.inputs["Distortion"].default_value = 40.0; scr.inputs["Detail"].default_value = 2.0; L.new(obj, scr.inputs["Vector"])
    L.new(_ramp(N, L, scr.outputs["Fac"], 0.97, 1.0), sc_.inputs["Height"])
    L.new(sc_.outputs["Normal"], b.inputs["Coat Normal"])
    base = _src(L, b.inputs["Base Color"])
    if base is not None:
        bw = N.new("ShaderNodeRGBToBW"); L.new(base, bw.inputs[0])
        rough = _value_of(N, L, b.inputs["Roughness"])
        L.new(_math(N, L, 'ADD', rough, _math(N, L, 'MULTIPLY', _ramp(N, L, bw.outputs[0], 0.15, 0.5), 0.18), clamp=True), b.inputs["Roughness"])


def hdri_world(sc, path=HDRI, fill_gain=1.4):
    """Light and reflections from a real outdoor HDRI; the camera still sees the track's own sky picture. The HDRI
    is turned so its sun sits where the track's sun lamp shines from, its sun disc is clipped (the lamp is the sun),
    and it is scaled to the brightness the matched sky fill had (track panorama mean x fill_gain)."""
    if not os.path.exists(path) or sc.world is None:
        log("hdri: none, sky picture kept"); return
    w = sc.world; N, L = w.node_tree.nodes, w.node_tree.links
    out = next(n for n in N if n.type == 'OUTPUT_WORLD')
    cam_bg = None
    for n in N:
        if n.type == 'BACKGROUND' and n.inputs["Color"].is_linked and _src(L, n.inputs["Color"]).node.type == 'TEX_ENVIRONMENT':
            cam_bg = n; break
    src = bpy.data.images.load(path, check_existing=True)
    ww, hh = src.size
    px = np.empty(ww * hh * 4, np.float32); src.pixels.foreach_get(px); px = px.reshape(hh, ww, 4)
    wl = np.array([0.2126, 0.7152, 0.0722], np.float32)
    lum = px[..., :3] @ wl
    y, x = np.unravel_index(np.argmax(lum), lum.shape)
    clip = float(np.percentile(lum, 99.0) * 2.0)
    px[..., :3] *= np.minimum(1.0, clip / np.maximum(lum, 1e-6))[..., None]
    img = bpy.data.images.new("REALISM_hdri", ww, hh, alpha=True, float_buffer=True)
    img.pixels.foreach_set(px.ravel())
    sun = bpy.data.objects.get("CINEMA_sun")
    s = (sun.matrix_world.to_3x3() @ Vector((0, 0, 1))).normalized() if sun else Vector((0, 0, 1))
    az_track = math.atan2(s.y, -s.x)                 # Blender equirect: u = atan2(y, -x) / 2pi + 0.5
    az_hdri = (x / ww - 0.5) * 2 * math.pi
    pano = bpy.data.images.get("track_sky")
    if pano is not None:
        pp = np.empty(len(pano.pixels), np.float32); pano.pixels.foreach_get(pp); pp = pp.reshape(-1, 4)[:, :3]
        target = float(np.where(pp <= 0.04045, pp / 12.92, ((pp + 0.055) / 1.055) ** 2.4).mean()) * fill_gain
    else:
        target = 0.5
    strength = target / max(1e-4, float((px[..., :3] @ wl).mean()))
    tc = N.new("ShaderNodeTexCoord"); mp = N.new("ShaderNodeMapping")
    mp.inputs["Rotation"].default_value = (0, 0, az_hdri - az_track)
    env = N.new("ShaderNodeTexEnvironment"); env.image = img
    L.new(tc.outputs["Generated"], mp.inputs[0]); L.new(mp.outputs[0], env.inputs[0])
    bg = N.new("ShaderNodeBackground"); bg.inputs["Strength"].default_value = strength; L.new(env.outputs[0], bg.inputs["Color"])
    lp = N.new("ShaderNodeLightPath"); mx = N.new("ShaderNodeMixShader")
    L.new(lp.outputs["Is Camera Ray"], mx.inputs[0]); L.new(bg.outputs[0], mx.inputs[1])
    L.new((cam_bg or bg).outputs[0], mx.inputs[2])
    L.new(mx.outputs[0], out.inputs["Surface"])
    log("hdri", os.path.basename(path), ww, "x", hh, "turned", round(math.degrees(az_hdri - az_track), 1), "deg, strength",
        round(strength, 3), "sun clipped above", round(clip, 2), "camera sees", cam_bg.name if cam_bg else "the hdri")


def _tyre(m):
    N, L = m.node_tree.nodes, m.node_tree.links
    b = _principled(m)
    obj = N.new("ShaderNodeTexCoord").outputs["Object"]
    r = N.new("ShaderNodeMapRange"); r.inputs["To Min"].default_value = 0.72; r.inputs["To Max"].default_value = 0.95
    L.new(_noise(N, L, 30.0, 6.0, 0.6, obj), r.inputs["Value"])
    L.new(r.outputs["Result"], b.inputs["Roughness"])
    b.inputs["Specular IOR Level"].default_value = 0.25


def apply(sc, frame, mud=True, hdri=True):
    O = bpy.data.objects
    f0 = int(math.floor(frame)); sc.frame_set(f0, subframe=frame - f0)
    _geometry()
    mud_img = None
    if mud and os.path.exists(MUD_TGA):
        mud_img = bpy.data.images.load(MUD_TGA, check_existing=True); mud_img.colorspace_settings.name = 'Non-Color'
        mud_img.alpha_mode = 'CHANNEL_PACKED'
    for m in bpy.data.materials:
        nm = m.name.lower()
        if not m.node_tree or _principled(m) is None:
            continue
        if nm.startswith("bike_") and any(s in nm for s in PLASTIC_SHEETS):
            _plastic(m, mud_img)
        elif m.get("cinema_tyre"):
            _tyre(m); log("tyre", m.name)
    z_low = O["chassis"].matrix_world.translation.z if "chassis" in O else 0.0
    arm = O.get("rider_rig")
    joints = [arm.matrix_world @ arm.pose.bones["riderRIG_" + n].head for n in
              ("LeftElbow", "RightElbow", "LeftKnee", "RightKnee", "LeftHip", "RightHip", "Spine1", "LeftShoulder", "RightShoulder")
              if arm is not None and ("riderRIG_" + n) in arm.pose.bones]
    for m in bpy.data.materials:
        nm = m.name.lower()
        if not m.node_tree or _principled(m) is None:
            continue
        if nm.startswith("cin_boots"):
            _dust(m, z_low, 0.55); log("dust", m.name)
        elif nm == "cin_body_rider":
            _fabric(m, joints, wrinkles=0.55); _dust(m, z_low, 0.18); log("fabric + wrinkles + dust", m.name, len(joints), "joints")
        elif nm == "cin_body_gloves":
            _fabric(m, grain=0.25); _dust(m, z_low + 0.3, 0.25); log("fabric + grain + dust", m.name)
        elif nm.startswith("cin_helmet_") and not any(k in nm for k in ("lens", "goggle", "crossbox")):
            _helmet_shell(m); log("helmet coat", m.name)
    if hdri:
        hdri_world(sc)


# ---------------------------------------------------------------- passes for an AI photo pass
def enable_passes(sc):
    vl = sc.view_layers[0]
    vl.use_pass_z = True; vl.use_pass_normal = True; vl.use_pass_object_index = True
    try:
        vl.use_pass_cryptomatte_object = True
    except Exception:
        pass
    for i, o in enumerate(bpy.data.objects):
        o.pass_index = i + 1


def _override(sc, kind):
    m = bpy.data.materials.new("REALISM_" + kind); m.use_nodes = True
    N, L = m.node_tree.nodes, m.node_tree.links
    for n in list(N):
        if n.type != 'OUTPUT_MATERIAL':
            N.remove(n)
    em = N.new("ShaderNodeEmission"); L.new(em.outputs[0], N["Material Output"].inputs["Surface"])
    if kind == "normal":
        g = N.new("ShaderNodeNewGeometry")
        vt = N.new("ShaderNodeVectorTransform"); vt.vector_type = 'NORMAL'; vt.convert_from = 'WORLD'; vt.convert_to = 'CAMERA'
        L.new(g.outputs["Normal"], vt.inputs[0])
        mad = N.new("ShaderNodeVectorMath"); mad.operation = 'MULTIPLY_ADD'
        mad.inputs[1].default_value = (0.5, 0.5, -0.5); mad.inputs[2].default_value = (0.5, 0.5, 0.5)
        L.new(vt.outputs[0], mad.inputs[0]); L.new(mad.outputs[0], em.inputs["Color"])
    elif kind == "depth":
        cd = N.new("ShaderNodeCameraData")
        r = N.new("ShaderNodeMapRange"); r.inputs["From Min"].default_value = sc["realism_near"]; r.inputs["From Max"].default_value = sc["realism_far"]
        r.inputs["To Min"].default_value = 1.0; r.inputs["To Max"].default_value = 0.0
        L.new(cd.outputs["View Z Depth"], r.inputs["Value"]); L.new(r.outputs["Result"], em.inputs["Color"])
    else:
        oi = N.new("ShaderNodeObjectInfo")
        hsv = N.new("ShaderNodeCombineColor"); hsv.mode = 'HSV'
        L.new(oi.outputs["Random"], hsv.inputs[0]); hsv.inputs[1].default_value = 0.75; hsv.inputs[2].default_value = 0.95
        L.new(hsv.outputs[0], em.inputs["Color"])
    return m


def ai_passes(sc, outdir, stem):
    """Depth (white = near), camera-space normal and object-ID colour images at the shot's resolution, sharp (no
    DOF, no motion blur), plus an edge image from depth + normal discontinuities, white lines on black."""
    os.makedirs(outdir, exist_ok=True)
    cam = sc.camera; eye = cam.matrix_world.translation
    hel = bpy.data.objects.get("helmet")
    d0 = (hel.matrix_world.translation - eye).length if hel else 5.0
    sc["realism_near"], sc["realism_far"] = max(0.1, d0 - 2.0), d0 + 6.0
    r, cy, vl = sc.render, sc.cycles, sc.view_layers[0]
    keep = dict(samples=cy.samples, den=cy.use_denoising, mb=r.use_motion_blur, dof=cam.data.dof.use_dof,
                vt=sc.view_settings.view_transform, look=sc.view_settings.look, ex=sc.view_settings.exposure,
                tr=r.film_transparent, fmt=r.image_settings.file_format, depth=r.image_settings.color_depth)
    cy.samples = 16; cy.use_denoising = False; r.use_motion_blur = False; cam.data.dof.use_dof = False
    sc.view_settings.view_transform = 'Standard'; sc.view_settings.look = 'None'; sc.view_settings.exposure = 0.0
    r.film_transparent = True; r.image_settings.file_format = 'PNG'; r.image_settings.color_depth = '16'
    out = {}
    for kind in ("depth", "normal", "objectid"):
        vl.material_override = _override(sc, kind)
        p = os.path.join(outdir, f"{stem}_{kind}.png"); r.filepath = p
        bpy.ops.render.render(write_still=True); out[kind] = p
        log("pass", kind, p)
    vl.material_override = None
    cy.samples = keep['samples']; cy.use_denoising = keep['den']; r.use_motion_blur = keep['mb']; cam.data.dof.use_dof = keep['dof']
    sc.view_settings.view_transform = keep['vt']; sc.view_settings.look = keep['look']; sc.view_settings.exposure = keep['ex']
    r.film_transparent = keep['tr']; r.image_settings.file_format = keep['fmt']; r.image_settings.color_depth = keep['depth']
    # edges
    def load(p):
        im = bpy.data.images.load(p); w, h = im.size
        a = np.empty(w * h * 4, np.float32); im.pixels.foreach_get(a); bpy.data.images.remove(im)
        return a.reshape(h, w, 4)
    dep = load(out["depth"]); nor = load(out["normal"]); idm = load(out["objectid"])
    def grad(x):
        gx = np.abs(np.diff(x, axis=1, prepend=x[:, :1])); gy = np.abs(np.diff(x, axis=0, prepend=x[:1]))
        return np.maximum(gx, gy)
    e = np.maximum(grad(dep[..., 0]) * 25.0, grad(nor[..., :3]).max(2) * 3.0)
    e = np.maximum(e, (grad(idm[..., :3]).max(2) > 0.02).astype(np.float32))
    e = np.where(dep[..., 3] > 0.5, e, 0.0)
    e = np.clip(e, 0, 1); e = (e > 0.35).astype(np.float32)
    h, w = e.shape
    im = bpy.data.images.new(f"{stem}_edges", w, h, alpha=False)
    px = np.ones((h, w, 4), np.float32); px[..., :3] = e[..., None]
    im.pixels.foreach_set(px.ravel()); p = os.path.join(outdir, f"{stem}_edges.png")
    im.filepath_raw = p; im.file_format = 'PNG'; im.save(); bpy.data.images.remove(im)
    out["edges"] = p; log("pass edges", p)
    return out


# ---------------------------------------------------------------- lens
def lens(path, barrel=0.012, ca=0.0012):
    """Slight barrel distortion and lateral chromatic aberration (red out, blue in) toward the corners."""
    im = bpy.data.images.load(path, check_existing=False); w, h = im.size
    px = np.empty(w * h * 4, np.float32); im.pixels.foreach_get(px); img = px.reshape(h, w, 4)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    nx = (xx - w / 2) / (w / 2); ny = (yy - h / 2) / (w / 2)
    r2 = nx * nx + ny * ny
    out = img.copy()
    for ch, k in ((0, 1 + ca), (1, 1.0), (2, 1 - ca)):
        s = (1 + barrel * r2) * k / (1 + barrel * 0.6)      # normalised so the frame stays filled
        sx = np.clip(nx * s * (w / 2) + w / 2, 0, w - 1.001); sy = np.clip(ny * s * (w / 2) + h / 2, 0, h - 1.001)
        x0 = sx.astype(np.int32); y0 = sy.astype(np.int32); fx = sx - x0; fy = sy - y0
        c = img[..., ch]
        out[..., ch] = (c[y0, x0] * (1 - fx) * (1 - fy) + c[y0, x0 + 1] * fx * (1 - fy) +
                        c[y0 + 1, x0] * (1 - fx) * fy + c[y0 + 1, x0 + 1] * fx * fy)
    im.pixels.foreach_set(out.ravel()); im.filepath_raw = path; im.file_format = 'PNG'; im.save(); bpy.data.images.remove(im)
    log("lens: barrel", barrel, "ca", ca)


def haze(path, depth_path, amount=0.07, colour=(0.80, 0.85, 0.92)):
    """Aerial haze on what is far (depth pass ~0 but drawn: the trees and track beyond the subject), not on the sky
    (transparent in the pass) or the subject."""
    def load(p):
        im = bpy.data.images.load(p, check_existing=False); w, h = im.size
        a = np.empty(w * h * 4, np.float32); im.pixels.foreach_get(a); bpy.data.images.remove(im)
        return a.reshape(h, w, 4), w, h
    img, w, h = load(path)
    dep, _, _ = load(depth_path)
    far = np.clip((0.06 - dep[..., 0]) / 0.06, 0, 1) * (dep[..., 3] > 0.5)
    s = 4
    sm = far[: h - h % s, : w - w % s].reshape(h // s, s, w // s, s).mean((1, 3))
    for _ in range(3):
        sm = (np.roll(sm, 1, 0) + np.roll(sm, -1, 0) + np.roll(sm, 1, 1) + np.roll(sm, -1, 1) + sm) / 5
    far = np.zeros((h, w), np.float32); far[: sm.shape[0] * s, : sm.shape[1] * s] = np.repeat(np.repeat(sm, s, 0), s, 1)
    a = (amount * far)[..., None]
    img[..., :3] = img[..., :3] * (1 - a) + np.array(colour, np.float32) * a
    im = bpy.data.images.load(path, check_existing=False); im.pixels.foreach_set(img.ravel()); im.filepath_raw = path
    im.file_format = 'PNG'; im.save(); bpy.data.images.remove(im)
    log("haze", amount, "on", round(float(far.mean()), 3), "of the frame")


def grade(path, contrast=1.07, saturation=1.07):
    """A gentle contrast (around mid grey) and saturation lift on the finished PNG: the HDRI fill and haze read a
    little washed next to the sky-picture light (Sean on v2: between v1 and v2)."""
    im = bpy.data.images.load(path, check_existing=False); w, h = im.size
    px = np.empty(w * h * 4, np.float32); im.pixels.foreach_get(px); img = px.reshape(h, w, 4)
    c = img[..., :3]
    c = np.clip(0.5 + (c - 0.5) * contrast, 0, 1)
    luma = (c * np.array([0.2126, 0.7152, 0.0722], np.float32)).sum(2, keepdims=True)
    img[..., :3] = np.clip(luma + saturation * (c - luma), 0, 1)
    im.pixels.foreach_set(img.ravel()); im.filepath_raw = path; im.file_format = 'PNG'; im.save(); bpy.data.images.remove(im)
    log("grade: contrast", contrast, "saturation", saturation)

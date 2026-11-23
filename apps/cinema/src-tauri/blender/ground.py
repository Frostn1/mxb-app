# MXB Cinema: the track ground material, data-driven from the track's own layer stack.
#
#   from ground import ground_materials
#   mat_bump, mat_disp = ground_materials(tj, tex_dir, load)
#
# tj      track.json (trackdump or the app's prep.rs: both carry "layers" and "textures")
# tex_dir folder of the raw sheets/masks/bumps named in tj
# load    load(name, path, w, h, ch=4, non_color=False, flipv=False) -> bpy image (the caller's img_from_raw)
#
# Colour is the game's, nothing else:
#   * every sheet is sampled as raw sRGB values (Non-Color) and the stack is blended exactly like the
#     game / the app's track viewer: layer 0 is the base; a later layer with a mask is mixed in by it,
#     one without a mask replaces what is below. Masks lie as stored (flipv), sheets tile per axis.
#   * the blended sRGB value is converted to linear once (exact sRGB curve) and is the albedo.
#     No grade, no hue shift, no overlay. In game a lit flat surface shows the sheet's own value.
#   * the only colour change is a tiny relative darkening (<= 6 %) in wet troughs/ruts.
# Everything else (ruts, chop, clods, grit, crumbs, roughness) is relief and roughness only:
#   Displacement output (true displacement on the detail patch, bump elsewhere) + roughness.
import os
import numpy as np


def _wh(lay, key, default):
    w, h = lay.get(key + "_w"), lay.get(key + "_h")
    if w and h:
        return w, h
    f = lay.get(key) or ""
    try:  # trackdump names carry the size: layer1_mask_8192x8192.r8
        dims = f.rsplit("_", 1)[1].split(".")[0].split("x")
        return int(dims[0]), int(dims[1])
    except Exception:
        return default, default


def ground_materials(tj, tex_dir, load, name="track_dirt"):
    import bpy
    texs = {t["name"]: t for t in tj.get("textures", [])}
    mat = bpy.data.materials.new(name); mat.use_nodes = True
    nt = mat.node_tree; N = nt.nodes; L = nt.links
    bsdf = N["Principled BSDF"]
    out = N["Material Output"]
    tc = N.new("ShaderNodeTexCoord")
    UV, OBJ = tc.outputs["UV"], tc.outputs["Object"]

    def mixc(a, b, fac=None, blend='MIX', const=None):
        m = N.new("ShaderNodeMix"); m.data_type = 'RGBA'; m.blend_type = blend
        if fac is None:
            m.inputs["Factor"].default_value = 1.0
        elif isinstance(fac, float):
            m.inputs["Factor"].default_value = fac
        else:
            L.new(fac, m.inputs["Factor"])
        L.new(a, m.inputs[6])
        if b is not None:
            L.new(b, m.inputs[7])
        else:
            m.inputs[7].default_value = const
        return m.outputs[2]

    def mixf(a, b, fac):
        m = N.new("ShaderNodeMix"); m.data_type = 'FLOAT'
        L.new(fac, m.inputs["Factor"]); L.new(a, m.inputs[2]); L.new(b, m.inputs[3])
        return m.outputs[0]

    col = hgt = nrm = None
    flat_n = N.new("ShaderNodeRGB"); flat_n.outputs[0].default_value = (0.5, 0.5, 1.0, 1.0)
    for i, lay in enumerate(tj.get("layers", [])):
        t = texs.get(lay["name"])
        if t:  # full-resolution sheet when the dump has one
            im = load("ground_" + lay["name"], os.path.join(tex_dir, t["file"]), t["w"], t["h"], non_color=True)
        else:
            sw, sh = _wh(lay, "sheet", 256)
            im = load("ground_" + lay["name"], os.path.join(tex_dir, lay["sheet"]), sw, sh, non_color=True)
        im.colorspace_settings.name = 'Non-Color'  # raw sRGB values, converted once below
        mp = N.new("ShaderNodeMapping"); mp.inputs["Scale"].default_value = (lay["tile"][0], lay["tile"][1], 1)
        L.new(UV, mp.inputs["Vector"])
        ti = N.new("ShaderNodeTexImage"); ti.image = im; ti.interpolation = 'Cubic'
        L.new(mp.outputs[0], ti.inputs["Vector"])
        c = ti.outputs["Color"]
        bw = N.new("ShaderNodeRGBToBW"); L.new(c, bw.inputs[0]); h = bw.outputs[0]
        # this layer's bump (tangent-space normal, rgb*2-1), tiled like the game does
        n_ = flat_n.outputs[0]
        if lay.get("bump"):
            bwid, bhei = _wh(lay, "bump", 512)
            bt_ = lay.get("bump_tile") or [1, 1]
            bim = load(f"ground_bump{i}_{lay['name']}", os.path.join(tex_dir, lay["bump"]), bwid, bhei,
                       non_color=True, flipv=True)  # rows as stored, like the viewer's DataTexture
            bim.colorspace_settings.name = 'Non-Color'
            bmp = N.new("ShaderNodeMapping"); bmp.inputs["Scale"].default_value = (bt_[0], bt_[1], 1)
            L.new(UV, bmp.inputs["Vector"])
            bti = N.new("ShaderNodeTexImage"); bti.image = bim
            L.new(bmp.outputs[0], bti.inputs["Vector"]); n_ = bti.outputs["Color"]
        if col is None or not lay.get("mask"):
            col, hgt, nrm = c, h, n_  # base layer, or a maskless layer that covers everything below
            continue
        mw, mh = _wh(lay, "mask", 8192)
        mk = load(f"ground_mask{i}_{lay['name']}", os.path.join(tex_dir, lay["mask"]), mw, mh, ch=1,
                  non_color=True, flipv=True)
        mk.colorspace_settings.name = 'Non-Color'
        mt = N.new("ShaderNodeTexImage"); mt.image = mk; mt.extension = 'EXTEND'
        L.new(UV, mt.inputs["Vector"])
        m = mt.outputs["Color"]
        col = mixc(col, c, m); hgt = mixf(hgt, h, m); nrm = mixc(nrm, n_, m)
    if col is None:  # a track with no stack: neutral mid dirt, still sRGB-encoded
        rgb = N.new("ShaderNodeRGB"); rgb.outputs[0].default_value = (0.55, 0.50, 0.44, 1)
        col = rgb.outputs[0]
        v = N.new("ShaderNodeValue"); v.outputs[0].default_value = 0.5; hgt = v.outputs[0]
        nrm = flat_n.outputs[0]

    # exact sRGB -> linear, once: ((c + 0.055) / 1.055) ^ 2.4
    a = mixc(col, None, blend='ADD', const=(0.055, 0.055, 0.055, 1))
    a = mixc(a, None, blend='MULTIPLY', const=(1 / 1.055, 1 / 1.055, 1 / 1.055, 1))
    gm = N.new("ShaderNodeGamma"); gm.inputs["Gamma"].default_value = 2.4
    L.new(a, gm.inputs["Color"]); albedo = gm.outputs[0]

    # ---- relief (metres) -> Displacement; it never touches colour
    def noise(scale, detail=6, rough=0.55, vec=None):
        t = N.new("ShaderNodeTexNoise")
        t.inputs["Scale"].default_value = scale; t.inputs["Detail"].default_value = detail
        t.inputs["Roughness"].default_value = rough
        L.new(vec or OBJ, t.inputs["Vector"])
        return t

    def mul_add(x, k, acc):
        m = N.new("ShaderNodeMath"); m.operation = 'MULTIPLY_ADD'; m.inputs[1].default_value = k
        L.new(x, m.inputs[0])
        if acc is None:
            m.inputs[2].default_value = 0.0
        else:
            L.new(acc, m.inputs[2])
        return m.outputs[0]

    def mrange(x, a0, a1, b0, b1, clamp=True):
        r = N.new("ShaderNodeMapRange"); r.clamp = clamp
        r.inputs[1].default_value = a0; r.inputs[2].default_value = a1
        r.inputs[3].default_value = b0; r.inputs[4].default_value = b1
        L.new(x, r.inputs[0])
        return r.outputs[0]

    warp = noise(1.8, 4)
    wv = N.new("ShaderNodeVectorMath"); wv.operation = 'MULTIPLY_ADD'; wv.inputs[1].default_value = (0.35, 0.35, 0.35)
    L.new(warp.outputs["Color"], wv.inputs[0]); L.new(OBJ, wv.inputs[2]); WOBJ = wv.outputs[0]
    # ruts along the riding direction
    rmap = N.new("ShaderNodeMapping"); rmap.inputs["Scale"].default_value = (0.35, 4.5, 1.0)
    L.new(WOBJ, rmap.inputs["Vector"])
    rut = mrange(noise(1.0, 3, 0.5, rmap.outputs[0]).outputs["Fac"], 0.38, 0.62, -1.0, 0.4)
    D = mul_add(rut, 0.028, None)
    # chop
    chop = N.new("ShaderNodeMath"); chop.operation = 'SUBTRACT'; chop.inputs[1].default_value = 0.5
    L.new(noise(3.2, 10, 0.62, WOBJ).outputs["Fac"], chop.inputs[0])
    D = mul_add(chop.outputs[0], 0.036, D)
    # clods where roost piled them
    clod = N.new("ShaderNodeTexVoronoi"); clod.inputs["Scale"].default_value = 22.0
    try:
        clod.feature = 'SMOOTH_F1'; clod.inputs["Smoothness"].default_value = 0.8
    except Exception:
        pass
    L.new(WOBJ, clod.inputs["Vector"])
    cl = N.new("ShaderNodeMath"); cl.operation = 'MULTIPLY'
    L.new(mrange(clod.outputs["Distance"], 0.0, 0.45, 1.0, 0.0), cl.inputs[0])
    L.new(mrange(noise(0.9, 3).outputs["Fac"], 0.48, 0.62, 0.0, 1.0), cl.inputs[1])
    D = mul_add(cl.outputs[0], 0.02, D)
    # stones/grit from the game's own sheets, and crumbs
    D = mul_add(hgt, 0.006, D)
    D = mul_add(noise(55.0, 4, 0.6).outputs["Fac"], 0.004, D)
    disp = N.new("ShaderNodeDisplacement"); disp.inputs["Midlevel"].default_value = 0.0; disp.inputs["Scale"].default_value = 1.0
    L.new(D, disp.inputs["Height"]); L.new(disp.outputs[0], out.inputs["Displacement"])

    # ---- moisture: troughs (Cycles pointiness) and ruts; drives roughness, and a <= 6 % relative darkening
    geo = N.new("ShaderNodeNewGeometry")
    cav = mrange(geo.outputs["Pointiness"], 0.47, 0.53, 1.0, 0.0)
    wet = N.new("ShaderNodeMath"); wet.operation = 'MULTIPLY'
    L.new(cav, wet.inputs[0]); L.new(noise(0.35, 6).outputs["Fac"], wet.inputs[1])
    wet = mul_add(wet.outputs[0], 1.3, None)
    rut_wet = mrange(rut, -1.0, 0.0, 1.0, 0.0)
    wmax = N.new("ShaderNodeMath"); wmax.operation = 'MAXIMUM'; wmax.use_clamp = True
    L.new(wet, wmax.inputs[0]); L.new(rut_wet, wmax.inputs[1]); W_ = wmax.outputs[0]
    shade = mrange(W_, 0.0, 1.0, 1.0, 0.94)  # 1.0 dry .. 0.94 wettest, grey: hue and saturation untouched
    sh = N.new("ShaderNodeCombineColor"); L.new(shade, sh.inputs[0]); L.new(shade, sh.inputs[1]); L.new(shade, sh.inputs[2])
    base = mixc(albedo, sh.outputs[0], blend='MULTIPLY')
    L.new(base, bsdf.inputs["Base Color"])
    rough = mrange(W_, 0.0, 1.0, 0.95, 0.72)
    rv = mrange(noise(5.0, 4).outputs["Fac"], 0.0, 1.0, -0.08, 0.08)
    rs = N.new("ShaderNodeMath"); rs.operation = 'ADD'; rs.use_clamp = True
    L.new(rough, rs.inputs[0]); L.new(rv, rs.inputs[1]); L.new(rs.outputs[0], bsdf.inputs["Roughness"])
    bsdf.inputs["Specular IOR Level"].default_value = 0.35
    # the game's own ground normals (per layer, through the masks)
    nm = N.new("ShaderNodeNormalMap"); nm.inputs["Strength"].default_value = 0.6
    L.new(nrm, nm.inputs["Color"]); L.new(nm.outputs[0], bsdf.inputs["Normal"])

    def _dm(m, v):
        try:
            m.displacement_method = v
        except Exception:
            m.cycles.displacement_method = v
    _dm(mat, 'BUMP')
    mat_d = mat.copy(); mat_d.name = name + "_displaced"; _dm(mat_d, 'BOTH')
    return mat, mat_d


def game_reference_material(tj, tex_dir, load, name="ground_game_reference", game_light=None):
    """What the game puts on screen: sheet * clamp(ambient + sun_color * max(N.L, 0), 0, 1), as emission
    (blended in sRGB like the game; render with the Standard view transform). game_light = (ambient, sun_color,
    sun_dir_blender) in the .amb's own units; None = lit 1 everywhere."""
    import bpy
    mat, _ = ground_materials(tj, tex_dir, load, name=name + "_tmp")
    nt = mat.node_tree; N = nt.nodes; L = nt.links
    gamma = next(n for n in N if n.type == 'GAMMA')
    # game blend: sRGB sheet value * lit, then to linear for the Standard view
    srgb = gamma.inputs["Color"].links[0].from_socket  # ((c+0.055)/1.055) chain input
    col_node = srgb.node.inputs[6].links[0].from_node.inputs[6].links[0].from_socket  # raw blended sRGB
    if game_light is not None:
        amb, sun, sdir = game_light
        geo = N.new("ShaderNodeNewGeometry")
        dp = N.new("ShaderNodeVectorMath"); dp.operation = 'DOT_PRODUCT'; dp.inputs[1].default_value = tuple(sdir)
        L.new(geo.outputs["Normal"], dp.inputs[0])
        mx = N.new("ShaderNodeMath"); mx.operation = 'MAXIMUM'; mx.inputs[1].default_value = 0.0
        L.new(dp.outputs["Value"], mx.inputs[0])
        lit = N.new("ShaderNodeMix"); lit.data_type = 'RGBA'; lit.blend_type = 'ADD'; lit.inputs["Factor"].default_value = 1.0
        sc_ = N.new("ShaderNodeMix"); sc_.data_type = 'RGBA'; sc_.blend_type = 'MULTIPLY'; sc_.inputs["Factor"].default_value = 1.0
        sc_.inputs[7].default_value = (*sun, 1)
        cmb = N.new("ShaderNodeCombineColor"); [L.new(mx.outputs[0], cmb.inputs[i]) for i in range(3)]
        L.new(cmb.outputs[0], sc_.inputs[6])
        lit.inputs[6].default_value = (*amb, 1); L.new(sc_.outputs[2], lit.inputs[7]); lit.clamp_result = True
        mul = N.new("ShaderNodeMix"); mul.data_type = 'RGBA'; mul.blend_type = 'MULTIPLY'; mul.inputs["Factor"].default_value = 1.0
        L.new(col_node, mul.inputs[6]); L.new(lit.outputs[2], mul.inputs[7])
        L.new(mul.outputs[2], srgb.node.inputs[6].links[0].from_node.inputs[6])
    em = N.new("ShaderNodeEmission"); L.new(gamma.outputs[0], em.inputs["Color"])
    L.new(em.outputs[0], N["Material Output"].inputs["Surface"])
    for l in list(N["Material Output"].inputs["Displacement"].links):
        L.remove(l)
    mat.name = name
    return mat

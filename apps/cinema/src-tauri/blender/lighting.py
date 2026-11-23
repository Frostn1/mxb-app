# MXB Cinema: the track's own lighting, the way the game lights it.
#
#   from lighting import game_lighting
#   info = game_lighting(scene, amb_text, collection, sky_color_socket_fn=None)
#
# The game lights a surface as  albedo * clamp(ambient + sun_color * max(N.L, 0), 0, 1)  with ambient and
# sun_color from the track's .amb ("clear" weather) and the sun direction from sun_position. The Cycles analog:
#   * world (every non-camera ray) = the ambient colour: a uniform sky of radiance A gives a diffuse
#     surface exactly albedo * A, from every side (minus what the scene occludes, plus real bounce);
#   * a sun of irradiance pi * sun_color gives albedo * sun_color * N.L.
# Both are scaled by one k so that flat sunlit ground comes out at 1.0 (the game's clamp), i.e. the ground
# shows its own sheet colour like in game and nothing is blown out on tracks whose .amb sums above 1.
# A soft fill from the camera's side (world light weighted toward the active camera, so it follows any
# camera without objects or handlers) keeps riders from going to silhouette when the sun is behind them.
import math
import re
from mathutils import Vector


def _block(txt, name):
    m = re.search(r"\b" + re.escape(name) + r"\s*\{", txt)
    if not m:
        return ""
    i, depth = m.end(), 1
    while i < len(txt) and depth:
        depth += {"{": 1, "}": -1}.get(txt[i], 0)
        i += 1
    return txt[m.end():i - 1]


def _vals(blk):
    out = {}
    for k, v in re.findall(r"\b(\w+)\s*=\s*([-0-9.eE]+)", blk):
        try:
            out[k] = float(v)
        except ValueError:
            pass
    return out


def amb_lighting(amb_txt):
    """(sun_dir_game, ambient_rgb, sun_rgb) from a track .amb; neutral defaults where it says nothing."""
    sp = _vals(_block(amb_txt, "sun_position"))
    sun_dir = Vector((sp.get("x", 1.0), sp.get("y", 2.0), sp.get("z", -1.0))).normalized()
    clear = _block(amb_txt, "clear") or amb_txt
    a = _vals(_block(clear, "ambient")); s = _vals(_block(clear, "sun_color"))
    amb = Vector((a.get("red", 0.45), a.get("green", 0.5), a.get("blue", 0.6)))
    sun = Vector((s.get("red", 1.0), s.get("green", 0.92), s.get("blue", 0.8)))
    return sun_dir, amb, sun


def game_lighting(sc, amb_txt, coll, sky_camera=None, sky_glossy=None, fill=0.22, name="CINEMA_sun"):
    import bpy
    sun_g, amb, sun = amb_lighting(amb_txt)
    sun_dir = Vector((-sun_g.x, -sun_g.z, sun_g.y)).normalized()  # game -> Blender (-x, -z, y)
    cos_flat = max(0.05, sun_dir.z)
    lit = amb + sun * cos_flat
    k = 1.0 / max(1e-3, max(lit))
    smax = max(sun) if max(sun) > 0 else 1.0
    # sun
    ld = bpy.data.lights.get(name) or bpy.data.lights.new(name, 'SUN')
    ld.type = 'SUN'
    ld.color = tuple(c / smax for c in sun)
    ld.energy = math.pi * k * smax
    ld.angle = math.radians(0.6)
    so = bpy.data.objects.get(name) or bpy.data.objects.new(name, ld)
    if so.name not in coll.objects:
        coll.objects.link(so)
    so.rotation_euler = sun_dir.to_track_quat('Z', 'Y').to_euler()
    # world
    w = sc.world or bpy.data.worlds.new("CINEMA_sky")
    sc.world = w; w.use_nodes = True
    N, L = w.node_tree.nodes, w.node_tree.links
    out = next(n for n in N if n.type == 'OUTPUT_WORLD')
    ambc = tuple(c * k for c in amb)
    bg_amb = N.new("ShaderNodeBackground"); bg_amb.inputs["Color"].default_value = (*ambc, 1); bg_amb.inputs["Strength"].default_value = 1.0
    # camera-side fill: world light weighted toward the active camera (Camera Data view vector is the
    # ray direction in camera space; +Z points back at the camera)
    cam = N.new("ShaderNodeCameraData")
    sep = N.new("ShaderNodeSeparateXYZ"); L.new(cam.outputs["View Vector"], sep.inputs[0])
    lobe = N.new("ShaderNodeMapRange"); lobe.clamp = True
    lobe.inputs[1].default_value = 0.2; lobe.inputs[2].default_value = 1.0
    L.new(sep.outputs["Z"], lobe.inputs[0])
    lum = 0.2126 * lit.x + 0.7152 * lit.y + 0.0722 * lit.z
    bg_fill = N.new("ShaderNodeBackground"); bg_fill.inputs["Color"].default_value = (1.0, 0.97, 0.93, 1)
    fs = N.new("ShaderNodeMath"); fs.operation = 'MULTIPLY'; fs.inputs[1].default_value = fill * k * lum * 3.0
    L.new(lobe.outputs[0], fs.inputs[0]); L.new(fs.outputs[0], bg_fill.inputs["Strength"])
    add = N.new("ShaderNodeAddShader"); L.new(bg_amb.outputs[0], add.inputs[0]); L.new(bg_fill.outputs[0], add.inputs[1])
    diffuse_side = add.outputs[0]
    lp = N.new("ShaderNodeLightPath")
    if sky_glossy is not None:  # reflections see the track's sky, at the ambient's brightness
        g = N.new("ShaderNodeBackground"); L.new(sky_glossy, g.inputs["Color"]); g.inputs["Strength"].default_value = k * max(amb) * 1.5
        mg = N.new("ShaderNodeMixShader"); L.new(lp.outputs["Is Glossy Ray"], mg.inputs[0])
        L.new(diffuse_side, mg.inputs[1]); L.new(g.outputs[0], mg.inputs[2]); diffuse_side = mg.outputs[0]
    if sky_camera is not None:
        mc = N.new("ShaderNodeMixShader"); L.new(lp.outputs["Is Camera Ray"], mc.inputs[0])
        L.new(diffuse_side, mc.inputs[1]); L.new(sky_camera, mc.inputs[2]); L.new(mc.outputs[0], out.inputs["Surface"])
    else:
        L.new(diffuse_side, out.inputs["Surface"])
    sc.cycles.diffuse_bounces = max(4, sc.cycles.diffuse_bounces)
    sc.cycles.max_bounces = max(8, sc.cycles.max_bounces)
    return {"sun_dir": sun_dir, "k": k, "ambient": ambc, "sun_energy": ld.energy, "lit_flat": tuple(lit * k)}


def sky_panorama(px):
    """The track's sky picture (H x W x 4 uint8, rows top-first) as a 2:1 equirect panorama, sky up.
    Returns (pano rows top-first, u of the sun, flip=False). A 4:1 dome (upper hemisphere, the game's
    dome_*.edf) is padded below the horizon with its own horizon colour, darkened; a 2:1 panorama stored
    upside down (sky_*.edf on some tracks) is turned the right way up by where its sky is."""
    import numpy as np
    h, w = px.shape[:2]
    a = px.copy()
    lum = a[..., :3].astype(np.float32).mean(-1)
    blue = a[..., 2].astype(np.float32) - a[..., 1].astype(np.float32) * 0.5 - a[..., 0].astype(np.float32) * 0.5
    if w / max(1, h) > 3.0:  # dome: upper hemisphere only; zenith row is the deeper blue one
        if blue[: h // 8].mean() < blue[-h // 8:].mean():
            a = a[::-1]
        hor = a[-max(1, h // 64):].astype(np.float32).mean(0)          # horizon band, per column
        below = np.repeat((hor * 0.55)[None], h, 0).astype(np.uint8)    # ground haze under the horizon
        a = np.concatenate([a, below], 0)
    else:  # full panorama: sky goes on top
        if blue[: h // 2].mean() < blue[h // 2:].mean():
            a = a[::-1]
    hh = a.shape[0] // 2
    l2 = a[:hh, :, :3].astype(np.float32).mean(-1)
    k = max(4, a.shape[1] // 50)
    c = np.cumsum(np.cumsum(np.pad(l2, ((k, k), (k, k)), mode="edge"), 0), 1)
    box = c[2 * k:, 2 * k:] - c[:-2 * k, 2 * k:] - c[2 * k:, :-2 * k] + c[:-2 * k, :-2 * k]
    u_sun = (float(np.unravel_index(np.argmax(box), box.shape)[1]) + 0.5) / a.shape[1]
    return a, u_sun, False


def ambient_scaled(amb_txt):
    """The world ambient game_lighting will use (the .amb ambient times its k), for materials that need it."""
    sun_g, amb, sun = amb_lighting(amb_txt)
    sdz = max(0.05, sun_g.y)  # game Y is up
    lit = amb + sun * sdz
    k = 1.0 / max(1e-3, max(lit))
    return tuple(c * k for c in amb)

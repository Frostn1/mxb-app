"""MXB Cinema "cinematic" preset: the matched shot (scene.py + outfit.py + pose.py, which reproduce the game's frame)
taken as a photographer would shoot it. Applied on top of a built, posed scene, for one camera and one frame:

    import cinematic
    cinematic.apply(sc, cam, frame, focus='helmet', follow='chassis')
    ... render ...
    cinematic.finish(png_path)

Everything here is a camera, light or finish choice; nothing touches the gear, bike or pose, so the matched preset stays
the baseline and this one is that shot made better.
  render   3840 x 2160, Cycles, adaptive sampling to SAMPLES, OptiX denoiser (albedo + normal passes)
  light    the track's own sky panorama (already turned to the .amb sun by scene.py) as the diffuse sky instead of a
           flat ambient colour, gain capped at 1.4 (matching the ambient's mean needed x1.9 and hazed the shadows); the sun softened to a
           2.5 deg disc (soft shadow edges); a warm, camera-invisible rim light behind and above the rider; more
           bounces so the rider and bike take light from the ground
  view     Standard, the matched render's (and the game's) colours: AgX kept the highlights but greyed the sky and the
           orange (tries 1-2). Rendered 0.3 EV under so nothing clips, then finish() lifts it back 0.35 EV with a soft
           shoulder from display 0.8 (Standard clips 1.4% of the rider/bike area at 0 EV) and a slight saturation lift
  lens     the matched camera's field of view on a 36 mm sensor, focused on the helmet; the aperture is the widest
           (from f/2.8) whose near limit still holds the nearest point of the bike and rider, so the bike stays sharp
           and the crowd and trees behind go soft (f/1.8 blurred the front wheel at ~2 m in the first try)
  motion   motion blur over 1/250 s from the recorded motion (1/50 s smeared the pitching rider): the wheels spin (their
           drivers read the keyed wheel_rot) and the camera pans with the rider (keyed on the helmet's travel), so the
           helmet holds still and the background streaks slightly
  finish   (on the saved PNG) a soft bloom on the highlights, a slight vignette, very light grain
"""
import math
import numpy as np
import bpy
from mathutils import Vector

SAMPLES = 384
F_STOP_MIN, F_STOP_MAX = 2.8, 11.0   # the aperture is solved per shot: the widest that keeps the whole bike sharp
COC_MM = 0.03                        # full-frame circle of confusion
SHUTTER_S = 1.0 / 250.0   # wheels and the far background move through it; rider and bike stay sharp
SUN_ANGLE_DEG = 2.5
RIM_POWER_W = 90.0
VIEW, LOOK = 'Standard', 'None'
EXPOSURE = -0.3          # rendered 0.3 EV under, so the suit and sky keep their top end in the 8-bit PNG ...
FINISH_EV = 0.35         # ... and finish() brings it back up through a soft shoulder
SHOULDER = 0.8           # display value where the highlight roll-off starts
SATURATION = 1.08
PIXEL_FILTER = 1.0       # Cycles' default 1.5 px Blackman-Harris softens 4K detail; 1.0 keeps it without aliasing
SKY_FILL_MAX = 1.4       # the panorama's gain cap: 1.9 (the ambient's mean) hazed the shadows, 1.0 left them muddy


def log(*a):
    print("CINEMATIC", *a, flush=True)


def _world_sky(sc):
    """The world's diffuse side from the track panorama (scaled to the matched ambient's mean) instead of one colour."""
    w = sc.world
    if w is None or w.node_tree is None:
        return
    N, L = w.node_tree.nodes, w.node_tree.links
    env = next((n for n in N if n.type == 'TEX_ENVIRONMENT' and n.image is not None), None)
    # the flat ambient (lighting.game_lighting's bg_amb): colour and strength both unlinked (the camera fill's
    # strength is driven, the camera-ray sky's colour is the panorama)
    amb = next((n for n in N if n.type == 'BACKGROUND' and not n.inputs["Color"].is_linked
                and not n.inputs["Strength"].is_linked), None)
    if env is None or amb is None:
        log("sky: no panorama in the world, ambient kept")
        return
    px = np.empty(len(env.image.pixels), np.float32)
    env.image.pixels.foreach_get(px)
    rgb = px.reshape(-1, 4)[:, :3]
    lin = np.where(rgb <= 0.04045, rgb / 12.92, ((rgb + 0.055) / 1.055) ** 2.4)
    mean = lin.mean(0)
    target = np.array(amb.inputs["Color"].default_value[:3]) * amb.inputs["Strength"].default_value
    gain = min(SKY_FILL_MAX, float((target.mean()) / max(1e-4, mean.mean())))
    mul = N.new("ShaderNodeMix"); mul.data_type = 'RGBA'; mul.blend_type = 'MULTIPLY'; mul.inputs["Factor"].default_value = 1.0
    mul.inputs["B"].default_value = (gain, gain, gain, 1)
    L.new(env.outputs["Color"], mul.inputs["A"]); L.new(mul.outputs["Result"], amb.inputs["Color"])
    amb.inputs["Strength"].default_value = 1.0
    log("sky: diffuse from the track panorama, gain", round(gain, 3))


def _area(sc, name, pos, target, power, size, color):
    ld = bpy.data.lights.new(name, 'AREA'); ld.shape = 'DISK'; ld.size = size
    ld.energy = power; ld.color = color
    o = bpy.data.objects.new(name, ld); sc.collection.objects.link(o)
    o.location = pos
    o.rotation_euler = (target - pos).to_track_quat('-Z', 'Y').to_euler()
    o.visible_camera = False; o.visible_glossy = False
    return o


def _rim(sc, cam, head, power=RIM_POWER_W, kicker=0.0):
    """Warm rim behind and above the rider (away from the camera); with `kicker` W, a second, low warm light from the
    side away from the camera that edges the bike and legs."""
    d = (head - cam.matrix_world.translation); d.z = 0; d.normalize()
    pos = head + d * 2.6 + Vector((0, 0, 1.4))
    _area(sc, "CINEMATIC_rim", pos, head, power, 1.6, (1.0, 0.93, 0.84))
    log("rim light", power, "W at", tuple(round(c, 2) for c in pos))
    if kicker > 0:
        side = d.cross(Vector((0, 0, 1))).normalized()
        kp = head + d * 1.2 + side * 2.4 + Vector((0, 0, -0.9))
        _area(sc, "CINEMATIC_kicker", kp, head - Vector((0, 0, 0.7)), kicker, 1.2, (1.0, 0.88, 0.72))
        log("kicker", kicker, "W at", tuple(round(c, 2) for c in kp))


def _pan(sc, cam, follow, frame, shutter_frames):
    """The camera pans with the rider over the shutter: keyed at nine instants across it (linear), moved by the
    focus object's own travel, so the helmet holds still in the frame. Following the chassis left the rider
    (who moves against the bike in the air) smeared by ~1.5 cm at 1/250 s, ~15-30 px at 4K (cinematic v3)."""
    f0 = int(math.floor(frame)); sub = frame - f0
    M = cam.matrix_world.copy()
    sc.frame_set(f0, subframe=sub); here = follow.matrix_world.translation.copy()
    for k in range(-4, 5):
        t = frame + k * shutter_frames / 8.0; fi = int(math.floor(t))
        sc.frame_set(fi, subframe=t - fi)
        cam.matrix_world = M.copy()
        cam.location = M.translation + (follow.matrix_world.translation - here)
        cam.keyframe_insert('location', frame=t)
    ad = cam.animation_data
    act = ad.action if ad else None
    curves = []
    if act is not None:
        if hasattr(act, 'fcurves'):
            curves = list(act.fcurves)
        else:                                          # layered actions (Blender 4.4+)
            for layer in act.layers:
                for strip in layer.strips:
                    for cb in strip.channelbags:
                        curves += list(cb.fcurves)
    for fc in curves:
        for kp in fc.keyframe_points:
            kp.interpolation = 'LINEAR'
    sc.frame_set(f0, subframe=sub)


def _subject_points(sc):
    pts = []
    for cn in ("bike", "rider"):
        coll = bpy.data.collections.get(cn)
        for o in (coll.all_objects if coll else []):
            if o.type == 'MESH' and not o.hide_render and len(o.data.vertices):
                pts += [o.matrix_world @ Vector(c) for c in o.bound_box]
    return pts


def _meshes_under(names, skip=("wheel",)):
    """Visible meshes that are, or hang under, the named objects (the rig's steer / fork carry the bars, plate, fork
    tubes as children), without the wheels."""
    out = []
    for o in bpy.data.objects:
        if o.type != 'MESH' or o.hide_render or any(k in o.name.lower() for k in skip):
            continue
        p = o
        while p is not None and p.name not in names:
            p = p.parent
        if p is not None:
            out.append(o)
    return out


def _close_points(sc):
    """The close photo's subject and what must stay sharp in it: the rider's upper body (helmet, spine, arms, hands)
    plus the bike's front (everything under steer and the fork: bars, number plate, fork; not the wheel). Sharp:
    the helmet, the hands and the bars, so the fork bottom and the rest may fall soft with the background."""
    O = bpy.data.objects
    frame_pts, sharp = [], []
    hel = O.get("helmet")
    if hel is not None:
        hp = [hel.matrix_world @ Vector(c) for c in hel.bound_box]; frame_pts += hp; sharp += hp
    for o in _meshes_under({"steer", "fsusp"}):
        bp = [o.matrix_world @ Vector(c) for c in o.bound_box]
        frame_pts += bp
        if o.parent is not None and o.parent.name == "steer" or o.name == "steer":
            sharp += bp
    arm = O.get("rider_rig")
    if arm is not None:
        for b in arm.pose.bones:
            n = b.name.replace("riderRIG_", "")
            if n in ("Spine1", "Spine2", "Spine3", "Spine4", "Neck1", "Head") or any(n.startswith(s + k) for s in ("Left", "Right")
                                                                                    for k in ("Collar", "Shoulder", "Elbow", "Wrist")):
                frame_pts.append(arm.matrix_world @ b.head)
                if n.endswith("Wrist"):
                    sharp.append(arm.matrix_world @ b.head)
    return frame_pts, sharp


def _tight_points(sc, reach=0.42):
    """The tight photo: helmet, hands, bars and number plate. The bars, plate and fork tubes are one mesh in the rig
    (under steer), so take its vertices within `reach` of the point between the hands (the grips)."""
    import numpy as np
    O = bpy.data.objects
    pts, sharp = [], []
    hel = O.get("helmet")
    if hel is not None:
        hp = [hel.matrix_world @ Vector(c) for c in hel.bound_box]; pts += hp; sharp += hp
    arm = O.get("rider_rig")
    hands = []
    if arm is not None:
        for nm in ("LeftWrist", "RightWrist", "LeftIndex3", "RightIndex3"):
            b = arm.pose.bones.get("riderRIG_" + nm)
            if b is not None:
                hands.append(arm.matrix_world @ b.tail)
    pts += hands
    if hands:
        mid = sum(hands, Vector()) / len(hands)
        dg = bpy.context.evaluated_depsgraph_get()
        for o in _meshes_under({"steer"}):
            e = o.evaluated_get(dg); me = e.to_mesh()
            co = np.empty(len(me.vertices) * 3, np.float32); me.vertices.foreach_get('co', co); e.to_mesh_clear()
            W = np.array(o.matrix_world, np.float32); co = co.reshape(-1, 3) @ W[:3, :3].T + W[:3, 3]
            near = co[np.linalg.norm(co - np.array(mid), axis=1) < reach]
            if len(near):
                lo, hi = near.min(0), near.max(0)
                pts += [Vector((x, y, z)) for x in (lo[0], hi[0]) for y in (lo[1], hi[1]) for z in (lo[2], hi[2])]
    return pts, sharp


def photo_camera(sc, frame, focus='helmet', follow='chassis', lens=70.0, height=-1.2, yaw_deg=40.0, margin=0.12, close=False, side='sun'):
    """A moto photographer's take on the same instant: a long lens from the 3/4 front, low and looking up so the sky
    sits behind the rider, on the side the sun lights. Framed on the whole bike + rider (their bounding boxes at this
    instant) with `margin` of the frame left on every side, at the distance that just fits them (the first try aimed
    at a point between helmet and chassis and cut the rear wheel of a nose-up bike). Returns the new camera."""
    O = bpy.data.objects
    f0 = int(math.floor(frame)); sc.frame_set(f0, subframe=frame - f0)
    ch = O[follow].matrix_world
    fwd = -(ch.to_3x3() @ Vector((0, 1, 0))); fwd.z = 0; fwd.normalize()     # the bike rides toward -Y (scene.py)
    lat = fwd.cross(Vector((0, 0, 1))).normalized()
    sun = O.get("CINEMA_sun")
    to_sun = (sun.matrix_world.to_3x3() @ Vector((0, 0, 1))).normalized() if sun else Vector((0, 0, 1))
    best = None
    for sgn in (1, -1):
        a = math.radians(yaw_deg)
        d = (fwd * math.cos(a) + lat * sgn * math.sin(a)).normalized()
        score = d.dot(to_sun) * (1 if side == 'sun' else -1)   # the sunlit side, or ('shade') the other one
        if best is None or score > best[0]:
            best = (score, d)
    d = best[1]
    if close == 'tight':
        pts, sharp_pts = _tight_points(sc)
    elif close:
        pts, sharp_pts = _close_points(sc)
    else:
        pts = sharp_pts = _subject_points(sc)
    if close:
        height *= 0.4                                   # closer in, a gentler look-up
    lo = Vector([min(p[i] for p in pts) for i in range(3)]); hi = Vector([max(p[i] for p in pts) for i in range(3)])
    target = (lo + hi) / 2
    tan_h = 18.0 / lens; tan_v = tan_h * sc.render.resolution_y / sc.render.resolution_x
    lim_h, lim_v = tan_h * (1 - 2 * margin), tan_v * (1 - 2 * margin)
    dist = 4.0
    for _ in range(400):                              # step back until every corner sits inside the margins
        pos = target + d * dist + Vector((0, 0, height))
        q = (target - pos).to_track_quat('-Z', 'Y'); R = q.to_matrix().inverted()
        ok = True
        for p in pts:
            v = R @ (p - pos)
            if v.z > -0.1 or abs(v.x / -v.z) > lim_h or abs(v.y / -v.z) > lim_v:
                ok = False; break
        if ok:
            break
        dist += 0.1
    hit, loc, *_ = sc.ray_cast(bpy.context.evaluated_depsgraph_get(), pos + Vector((0, 0, 50)), Vector((0, 0, -1)))
    if hit and pos.z < loc.z + 0.8:
        pos.z = loc.z + 0.8                            # a photographer crouched on the ground, not under it
    cd = bpy.data.cameras.new("CAM_photo"); cd.sensor_fit = 'HORIZONTAL'; cd.sensor_width = 36.0; cd.lens = lens
    cd.clip_start = 0.05; cd.clip_end = 5000
    cam = bpy.data.objects.new("CAM_photo", cd); sc.collection.objects.link(cam)
    cam.location = pos
    cam.rotation_euler = (target - pos).to_track_quat('-Z', 'Y').to_euler()
    bpy.context.view_layer.update()                    # matrix_world for apply()'s focus and aperture maths
    cam["cinematic_sharp_points"] = [list(p) for p in sharp_pts]   # what apply() keeps inside the depth of field
    log("photo camera", close if isinstance(close, str) else ("close" if close else "full"), lens, "mm at", tuple(round(c, 2) for c in pos), "distance", round((target - pos).length, 2),
        "subject box", tuple(round(c, 2) for c in (hi - lo)), "sun side score", round(best[0], 2))
    return cam


def apply(sc, cam, frame, focus='helmet', follow='chassis', rim_power=RIM_POWER_W, kicker=0.0, samples=SAMPLES, fstop=None, fill=0.0):
    O = bpy.data.objects
    r = sc.render; cy = sc.cycles
    r.engine = 'CYCLES'; r.resolution_x, r.resolution_y, r.resolution_percentage = 3840, 2160, 100
    cy.samples = samples; cy.use_adaptive_sampling = True; cy.adaptive_threshold = 0.01 if samples <= SAMPLES else 0.005
    cy.use_denoising = True
    try:
        cy.denoiser = 'OPTIX'
    except Exception:
        pass
    cy.denoising_input_passes = 'RGB_ALBEDO_NORMAL'
    cy.max_bounces = 12; cy.diffuse_bounces = 6; cy.glossy_bounces = 6
    cy.filter_width = PIXEL_FILTER
    sc.view_settings.view_transform = VIEW
    try:
        sc.view_settings.look = LOOK
    except Exception:
        pass
    sc.view_settings.exposure = EXPOSURE
    sun = O.get("CINEMA_sun")
    if sun is not None:
        sun.data.angle = math.radians(SUN_ANGLE_DEG)
    _world_sky(sc)
    head = O[focus].matrix_world.translation.copy() if focus in O else cam.matrix_world.translation
    _rim(sc, cam, head, rim_power, kicker)
    if fill > 0:
        # a big soft source beside the camera: a photographer's reflector / fill for a shade-side shot
        eye = cam.matrix_world.translation
        side_v = (head - eye).cross(Vector((0, 0, 1))).normalized()
        fp = eye + (head - eye) * 0.35 + side_v * 1.2 + Vector((0, 0, 0.8))
        _area(sc, "CINEMATIC_fill", fp, head, fill, 2.5, (1.0, 0.98, 0.95))
        log("fill", fill, "W at", tuple(round(c, 2) for c in fp))
    # lens: the matched field of view on a full-frame sensor, focused on the helmet
    cd = cam.data; hfov = cd.angle
    cd.sensor_fit = 'HORIZONTAL'; cd.sensor_width = 36.0; cd.angle = hfov
    if focus in O:
        cd.dof.focus_object = O[focus]
    eye = cam.matrix_world.translation
    dist = (head - eye).length
    near = far = dist
    if "cinematic_sharp_points" in cam:               # a framed subject (photo_camera): only it must be sharp
        for p in cam["cinematic_sharp_points"]:
            dd = (Vector(p) - eye).length; near = min(near, dd); far = max(far, dd)
    else:
        for cn in ("bike", "rider"):
            coll = bpy.data.collections.get(cn)
            for o in (coll.all_objects if coll else []):
                if o.type == 'MESH' and not o.hide_render:
                    for c in o.bound_box:
                        near = min(near, (o.matrix_world @ Vector(c) - eye).length)
    near = max(0.3, near * 0.95)                      # a little margin in front of the nearest part
    f_m = cd.lens / 1000.0
    H = near * dist / max(1e-3, dist - near)          # the hyperfocal distance whose near limit is `near`
    fstop = fstop or min(F_STOP_MAX, max(F_STOP_MIN, f_m * f_m / (H * COC_MM / 1000.0)))   # or the shot's own
    cd.dof.use_dof = True; cd.dof.aperture_fstop = fstop
    # motion
    fps = sc.render.fps / sc.render.fps_base
    r.use_motion_blur = True; r.motion_blur_shutter = SHUTTER_S * fps; r.motion_blur_position = 'CENTER'
    pan = focus if focus in O else follow             # hold the rider still; the bike follows within a few mm
    if pan in O:
        _pan(sc, cam, O[pan], frame, r.motion_blur_shutter)
    log("lens", round(cd.lens, 1), "mm f/%.1f focus %.2f m, nearest part %.2f m" % (fstop, dist, near), "shutter frames", round(r.motion_blur_shutter, 2),
        "samples", samples, "denoiser", getattr(cy, 'denoiser', '?'))


def _blur(a, r):
    """Separable box blur, three passes (close to a gaussian), on an HxWxC float array."""
    for _ in range(3):
        for ax in (0, 1):
            c = np.cumsum(np.pad(a, [(r + 1, r) if i == ax else (0, 0) for i in range(a.ndim)], mode='edge'), axis=ax)
            hi = np.take(c, np.arange(2 * r + 1, c.shape[ax]), axis=ax); lo = np.take(c, np.arange(0, c.shape[ax] - 2 * r - 1), axis=ax)
            a = (hi - lo) / (2 * r + 1)
    return a


def finish(path, bloom=0.05, vignette=0.15, grain=0.010, seed=7, ev=None, shoulder=None, saturation=None):
    im = bpy.data.images.load(path, check_existing=False)
    w, h = im.size
    px = np.empty(w * h * 4, np.float32); im.pixels.foreach_get(px)
    img = px.reshape(h, w, 4)[:, :, :3].astype(np.float32)
    ev = FINISH_EV if ev is None else ev; shoulder = SHOULDER if shoulder is None else shoulder
    saturation = SATURATION if saturation is None else saturation
    if ev:
        # exposure in linear light, back to display, then the highlight shoulder (y = x below it, an exponential
        # approach to 1 above it, continuous in value and slope)
        lin = np.where(img <= 0.04045, img / 12.92, ((img + 0.055) / 1.055) ** 2.4) * (2.0 ** ev)
        img = np.where(lin <= 0.0031308, lin * 12.92, 1.055 * np.power(np.maximum(lin, 0), 1 / 2.4) - 0.055)
        k = 1.0 - shoulder
        img = np.where(img <= shoulder, img, shoulder + k * (1.0 - np.exp(-(img - shoulder) / k))).astype(np.float32)
    if saturation != 1.0:
        luma = (img * np.array([0.2126, 0.7152, 0.0722], np.float32)).sum(2, keepdims=True)
        img = np.clip(luma + saturation * (img - luma), 0, 1)
    # bloom: what is near white, blurred wide at quarter resolution, added back softly
    s = 4
    small = img[: h - h % s, : w - w % s].reshape(h // s, s, w // s, s, 3).mean((1, 3))
    hl = np.clip((small - 0.92) / 0.08, 0, 1) * small     # only the true highlights; a lower knee hazed the sky
    gl = _blur(hl, 6) * 0.6 + _blur(hl, 20) * 0.4
    gl = np.repeat(np.repeat(gl, s, 0), s, 1)
    out = img.copy(); out[: gl.shape[0], : gl.shape[1]] += bloom * gl
    # vignette: a soft fall-off toward the corners
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    rr = (((xx - w / 2) / (w / 2)) ** 2 + ((yy - h / 2) / (h / 2)) ** 2) / 2.0
    out *= (1.0 - vignette * np.clip(rr, 0, 1) ** 1.5)[..., None]
    # grain: fine luminance noise, strongest in the mid-tones
    rng = np.random.default_rng(seed)
    lum = out.mean(2, keepdims=True)
    out += grain * rng.standard_normal((h, w, 1)).astype(np.float32) * (4 * lum * (1 - lum)).clip(0, 1)
    px.reshape(h, w, 4)[:, :, :3] = np.clip(out, 0, 1)
    im.pixels.foreach_set(px); im.filepath_raw = path; im.file_format = 'PNG'; im.save()
    bpy.data.images.remove(im)
    log("finish: ev", ev, "shoulder", shoulder, "saturation", saturation, "bloom", bloom, "vignette", vignette, "grain", grain)

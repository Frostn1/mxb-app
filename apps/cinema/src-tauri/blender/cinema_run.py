"""MXB Cinema: the whole shot in one Blender process.

  blender -b --factory-startup -t 2 -P gpu.py -P cinema_run.py -- --job <shot>/job.json

job.json (written by the app, export.rs) names the clip (replay JSON v1, SCHEMA.md), the prepared track, bike and
rider folders, where the .blend goes and what to render. The stages run in order in this one process:

  scene    build the track, bike, rider, light and sky                     scene.py
  animate  key the bike from the clip, place the wide cameras               animate.py (+ cameras.py)
  drive    steer, fork, swingarm (through the .geom linkage), wheel spin    drive_scene.py
  contact  tyre contact patch on the ground                                 contact.py
  outfit   the rider's own gear and paints                                  outfit.py
  pose     the rider's body from the telemetry, hands and feet on the bike  pose.py
  fx       roost, dust, landing puffs, tyre marks                           fx.py
  shots    the tight cameras (chase, trackside, detail) and render setup    shots.py
  save     the .blend, opening on the marked frame, textures beside it
  render   a still or a clip, when asked

A stage that fails stops the job: nothing is retried."""
import bpy, sys, os, json, time, traceback, math

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
from cinema_job import JOB, JOB_FILE, log  # noqa: E402

t_start = time.time()
clip = json.load(open(JOB['clip']))
mark = int(clip.get('mark_frame', 1))
rider = bool(JOB.get('rider_dir'))
# The arguments every stage script reads (they parse what follows `--`).
args = ['--job', JOB_FILE, '--json', JOB['clip'], '--sa-ref', '0', '--lean-gain', '0', '--mode', 'none',
        '--samples', str(JOB.get('samples', 128)), '--res', str(JOB['res'][0]), str(JOB['res'][1])]
if not rider:
    args.append('--no-rider')
sys.argv = [sys.argv[0], '--'] + args

stages = [('scene', 'scene.py'), ('animate', 'animate.py'), ('drive', 'drive_scene.py'), ('contact', 'contact.py')]
if rider:
    stages += [('outfit', 'outfit.py'), ('pose', 'pose.py')]
if JOB.get('fx', True):
    stages.append(('fx', 'fx.py'))
stages.append(('shots', 'shots.py'))


def run_stage(name, file):
    log('STEP', name)
    path = os.path.join(HERE, file)
    t0 = time.time()
    g = {'__file__': path, '__name__': '__main__'}
    exec(compile(open(path, encoding='utf-8').read(), path, 'exec'), g)
    log('done', name, round(time.time() - t0, 1), 's')
    if name == 'outfit':  # the rider's gear maps, read the way they were authored (nmapfix.py)
        from nmapfix import fix_normal_maps
        c = bpy.data.collections.get('rider')
        if c:
            fix_normal_maps(c.all_objects, log=log)


try:
    for name, file in stages:
        run_stage(name, file)
    sc = bpy.context.scene
    # The .blend opens on the marked frame, through the camera the shot is about.
    cams = {k: bpy.data.objects.get(v) for k, v in (('chase', 'CAM_chase'), ('trackside', 'CAM_trackside_low'), ('detail', 'CAM_detail'))}
    want = JOB.get('camera', 'auto')
    if want == 'auto':
        apex = int(sc.get('cinema_apex_frame', 0))
        air = clip['riders'][0]['frames'][mark - 1].get('airborne', False)
        want = 'trackside' if air and apex and abs(apex - mark) < 40 else 'chase'
    cam = cams.get(want) or cams.get('chase') or sc.camera
    vc = JOB.get('view_camera')
    if vc:
        # The app's preview camera. Its frame is the world's with x mirrored (TrackViewer toView), so
        # view -> Blender is (x, -z, y), a proper rotation; a camera looks down -Z with +Y up in both.
        from mathutils import Matrix, Quaternion, Vector
        V2B = Matrix(((1, 0, 0), (0, 0, -1), (0, 1, 0)))
        qx, qy, qz, qw = vc['quaternion']
        R = V2B @ Quaternion((qw, qx, qy, qz)).to_matrix()
        p = vc['position']
        cd = bpy.data.cameras.new('CAM_view')
        cd.sensor_fit = 'VERTICAL'
        cd.angle_y = math.radians(float(vc.get('fov', 45)))
        cd.clip_start = 0.05
        cd.clip_end = 3000
        cam = bpy.data.objects.new('CAM_view', cd)
        (bpy.data.collections.get('cameras') or sc.collection).objects.link(cam)
        cam.matrix_world = Matrix.Translation(Vector((-p[0], -p[2], p[1]))) @ R.to_4x4()
        for m in list(sc.timeline_markers):
            sc.timeline_markers.remove(m)
        sc.camera = cam
        log('view camera at', tuple(round(x, 2) for x in cam.location))
    sc.frame_set(mark)
    sc['cinema_mark_frame'] = mark
    if JOB['render'] == 'still' and cam:
        # one still from one camera: no cuts between cameras
        for m in list(sc.timeline_markers):
            sc.timeline_markers.remove(m)
        sc.camera = cam
    # The rider as a solid object at the shot's frame: the skinned meshes are posed into plain meshes
    # parented to the rider root, so moving the rider moves it whole; the rig is kept, hidden.
    rr = bpy.data.objects.get('CINEMA_rider_root')
    if rr is not None:
        sc.frame_set(mark)
        dg = bpy.context.evaluated_depsgraph_get()
        riders = bpy.data.collections.get('rider')
        rigc = bpy.data.collections.new('Rider rig (hidden)')
        riders.children.link(rigc)
        baked = 0
        for o in list(riders.all_objects):
            if o.type != 'MESH' or not any(m.type == 'ARMATURE' for m in o.modifiers):
                continue
            me = bpy.data.meshes.new_from_object(o.evaluated_get(dg), preserve_all_data_layers=True, depsgraph=dg)
            b = bpy.data.objects.new(o.name + ' (posed)', me)
            riders.objects.link(b)
            b.matrix_world = o.matrix_world.copy()
            w = b.matrix_world.copy()
            b.parent = rr
            b.matrix_parent_inverse = rr.matrix_world.inverted()
            b.matrix_world = w
            for c in list(o.users_collection):
                c.objects.unlink(o)
            rigc.objects.link(o)
            o.hide_viewport = True
            o.hide_render = True
            baked += 1
        arm = bpy.data.objects.get('rider_rig')
        if arm is not None:
            arm.hide_viewport = False
            arm.hide_set(True)
            arm.name = 'Rider rig'
        rr.name = 'Rider (move me)'
        rr.empty_display_size = 0.6
        log('rider baked at frame', mark, baked, 'meshes')
    # Opening the .blend shows what the render shows: through the shot's camera, with the scene's
    # own sky and sun rather than the viewport's studio light.
    for scr in bpy.data.screens:
        for area in scr.areas:
            if area.type != 'VIEW_3D':
                continue
            for sp in area.spaces:
                if sp.type != 'VIEW_3D':
                    continue
                sh = sp.shading
                sh.type = 'MATERIAL'
                sh.use_scene_world = True
                sh.use_scene_lights = True
                sh.use_scene_world_render = True
                sh.use_scene_lights_render = True
                sp.clip_end = 5000
                if sp.region_3d is not None:
                    sp.region_3d.view_perspective = 'CAMERA'
    log('STEP', 'save')
    bpy.context.preferences.filepaths.save_version = 0   # no shot.blend1 beside it
    bpy.ops.wm.save_as_mainfile(filepath=JOB['out_blend'], compress=True)
    # textures beside the .blend, by relative path, so the shot folder moves as one
    bpy.ops.file.make_paths_relative()
    bpy.ops.wm.save_mainfile(compress=True)
    log('saved', JOB['out_blend'])
    r = sc.render
    r.resolution_x, r.resolution_y = JOB['res']
    r.resolution_percentage = 100
    sc.cycles.samples = int(JOB.get('samples', 128))
    if JOB['render'] == 'still':
        log('STEP', 'render')
        r.image_settings.file_format = 'PNG'
        r.filepath = JOB['out_still']
        sc.frame_set(mark)
        look = JOB.get('look', 'matched')
        shot = JOB.get('shot', 'view')
        real = look == 'realism'
        # cinematic.py and realism.py find the rig by its working name; the .blend has been saved
        arm = bpy.data.objects.get('Rider rig')
        if arm is not None:
            arm.name = 'rider_rig'
        if shot in ('photo', 'close', 'tight') or look != 'matched':
            import cinematic
        if shot in ('photo', 'close', 'tight'):
            # a photographer's camera at the same instant: whole bike, rider + bars, or helmet + hands (cinematic.py)
            for m in list(sc.timeline_markers):
                sc.timeline_markers.remove(m)
            sc.camera = cinematic.photo_camera(sc, mark, close={'photo': False, 'close': True, 'tight': 'tight'}[shot],
                                               side='shade' if JOB.get('side') == 'shade' else 'sun')
        if look != 'matched' and sc.camera is not None:
            # the matched shot made better: soft sun + sky light + rim, depth of field on the helmet, motion blur
            # with a panning camera, then bloom / vignette / grain (cinematic.py). Background blur: f/11 at 0 to
            # f/2 at 1; motion blur: off at 0, else a 1/1000 to 1/60 s shutter.
            blur = float(JOB.get('blur', 0.8))
            motion = float(JOB.get('motion', 0.5))
            if motion > 0:
                cinematic.SHUTTER_S = (1 / 1000) * (1000 / 60) ** motion
            photo = shot != 'view'
            shade = photo and JOB.get('side') == 'shade'      # backlit: stronger rim and kicker, a soft fill beside the camera
            cinematic.apply(sc, sc.camera, mark, rim_power=(260.0 if shade else 220.0) if photo else cinematic.RIM_POWER_W,
                            kicker=(160.0 if shade else 120.0) if photo else 0.0, samples=int(JOB.get('samples', 128)),
                            fstop=11.0 * (2.0 / 11.0) ** blur, fill=220.0 if shade else 0.0)
            if motion <= 0:
                r.use_motion_blur = False
            r.resolution_x, r.resolution_y = JOB['res']     # apply() sets 4K; the job's size wins
            if real:
                # subdivided silhouettes, coated + scuffed + muddy plastics, dusty gear, fabric sheen, light from a
                # real outdoor HDRI (realism.py); Mud scales the bike's mud and the gear's dust together
                import realism
                if JOB.get('mud_map'):
                    realism.MUD_TGA = JOB['mud_map']
                realism.MUD_AMOUNT = float(JOB.get('mud', 1.0))
                # the rendered rider is the baked copy: it takes the name realism.py subdivides by
                for nm in realism.SUBDIV:
                    o, b = bpy.data.objects.get(nm), bpy.data.objects.get(nm + ' (posed)')
                    if o is not None and b is not None:
                        o.name = nm + ' (rig)'
                        b.name = nm
                realism.apply(sc, mark, hdri=False)
                if JOB.get('hdri'):
                    realism.hdri_world(sc, path=JOB['hdri'])
                else:
                    realism.hdri_world(sc)
            log('look', look, 'shot', shot, 'side', JOB.get('side'), 'blur', blur, 'motion', motion, 'mud', JOB.get('mud') if real else '-')
        bpy.ops.render.render(write_still=True)
        if look != 'matched':
            cinematic.finish(JOB['out_still'])
            if real:
                realism.grade(JOB['out_still'])
                realism.lens(JOB['out_still'])
        log('still', JOB['out_still'])
    elif JOB['render'] == 'clip':
        log('STEP', 'render')
        try:
            r.image_settings.media_type = 'VIDEO'
        except Exception:
            pass
        r.image_settings.file_format = 'FFMPEG'
        r.ffmpeg.format = 'MPEG4'
        r.ffmpeg.codec = 'H264'
        r.ffmpeg.constant_rate_factor = 'HIGH'
        r.filepath = JOB['out_clip']
        r.use_file_extension = False
        bpy.ops.render.render(animation=True)
        log('clip', JOB['out_clip'])
    log('STEP', 'done')
    log('total', round(time.time() - t_start, 1), 's')
except SystemExit:
    raise
except Exception:
    traceback.print_exc()
    log('ERROR', 'stageFailed')
    sys.stdout.flush()
    sys.exit(1)
sys.stdout.flush()
# Leave right away: nothing else is to run in this process.
os._exit(0)

"""MXB Cinema integration: animate a scene from replay JSON, add cameras, render.
blender -b [scene.blend] -P animate.py -- --json X.json [--root NAME] [--out DIR] [--render] [--still FRAME]
   [--save out.blend] [--res 1920 1080] [--frames A B] [--placeholder] [--name clip]
The root object (default CINEMA_bike_root) gets loc/rot keys plus keyed custom props
steer/fork/shock/wheel_rot_f/wheel_rot_r for the rig's drivers."""
import bpy, json, sys, math, os, argparse
from mathutils import Matrix, Vector

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
ap = argparse.ArgumentParser()
ap.add_argument('--json', required=True)
ap.add_argument('--root', default='CINEMA_bike_root')
ap.add_argument('--out', default='')
ap.add_argument('--still', type=int)
ap.add_argument('--save')
ap.add_argument('--res', type=int, nargs=2, default=[1920, 1080])
ap.add_argument('--frames', type=int, nargs=2)
ap.add_argument('--placeholder', action='store_true')
ap.add_argument('--render', action='store_true')
ap.add_argument('--name', default='clip')
ap.add_argument('--samples', type=int, default=64)
ap.add_argument('--eevee-samples', type=int, default=32)
a, _ = ap.parse_known_args(argv)

D = json.load(open(a.json))
F = D['riders'][0]['frames']
M = Matrix(D['to_blender'])
sc = bpy.context.scene
sc.render.fps = D['fps']
sc.frame_start = 1
sc.frame_end = len(F)
PROPS = ('steer', 'fork', 'shock', 'wheel_rot_f', 'wheel_rot_r')


def coll(name):
    c = bpy.data.collections.get(name) or bpy.data.collections.new(name)
    if c.name not in sc.collection.children:
        sc.collection.children.link(c)
    return c


C = coll('CINEMA')
root = bpy.data.objects.get(a.root)
if root is None:
    root = bpy.data.objects.new(a.root, None)
    root.empty_display_type = 'ARROWS'
    C.objects.link(root)
if root.animation_data:
    root.animation_data_clear()

if a.placeholder and not bpy.data.objects.get('PLACEHOLDER_bike'):
    bpy.ops.mesh.primitive_cube_add(size=1)
    b = bpy.context.object
    b.name = 'PLACEHOLDER_bike'
    b.scale = (0.35, 1.3, 0.5)
    b.location = (0, 0, 0.6)
    b.parent = root
    for i, y in enumerate((0.72, -0.72)):
        bpy.ops.mesh.primitive_cylinder_add(radius=0.36, depth=0.12, rotation=(0, math.pi / 2, 0), location=(0, y, 0.36))
        w = bpy.context.object
        w.parent = root
        w.name = f'PLACEHOLDER_wheel{i}'
        # spoke marker so spin is visible
        bpy.ops.mesh.primitive_cube_add(size=1, location=(0.07, y, 0.36 + 0.2))
        s = bpy.context.object
        s.scale = (0.02, 0.05, 0.36)
        s.parent = w
        s.matrix_parent_inverse = w.matrix_world.inverted()

root.rotation_mode = 'QUATERNION'
for k in PROPS:
    root[k] = 0.0
# EDF meshes are imported with the same M, so the chassis world rotation in Blender is M R M^T.
prev = None
for i, f in enumerate(F):
    fr = i + 1
    R = Matrix(f['rot'])
    q = (M @ R @ M.transposed()).to_quaternion()
    if prev is not None and prev.dot(q) < 0:
        q.negate()
    prev = q
    root.location = M @ Vector(f['pos'])
    root.rotation_quaternion = q
    root.keyframe_insert('location', frame=fr)
    root.keyframe_insert('rotation_quaternion', frame=fr)
    root['steer'] = f['steer']
    root['fork'] = f['fork']
    root['shock'] = f['shock']
    root['wheel_rot_f'], root['wheel_rot_r'] = f['wheel_rot']
    for k in PROPS:
        root.keyframe_insert(f'["{k}"]', frame=fr)

if a.placeholder:
    for i, k in enumerate(('wheel_rot_f', 'wheel_rot_r')):
        w = bpy.data.objects[f'PLACEHOLDER_wheel{i}']
        w.rotation_mode = 'XYZ'
        w.driver_remove('rotation_euler', 0)
        d = w.driver_add('rotation_euler', 0).driver
        v = d.variables.new()
        v.targets[0].id = root
        v.targets[0].data_path = f'["{k}"]'
        d.expression = '-var'

exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)),'cameras.py')).read())

# ---- sky if the scene has none ------------------------------------------------------------------
w = sc.world
has_sky = w is not None and w.use_nodes and any(nd.type in ('TEX_ENVIRONMENT', 'TEX_SKY') for nd in w.node_tree.nodes)
if not has_sky:
    w = w or bpy.data.worlds.new('CINEMA_sky')
    sc.world = w
    w.use_nodes = True
    nt = w.node_tree
    for nd in list(nt.nodes):
        if nd.type != 'OUTPUT_WORLD':
            nt.nodes.remove(nd)
    out = [x for x in nt.nodes if x.type == 'OUTPUT_WORLD'][0]
    sky = nt.nodes.new('ShaderNodeTexSky')
    bg = nt.nodes.new('ShaderNodeBackground')
    for attr, val in (('sun_elevation', math.radians(30)), ('sun_rotation', math.radians(140))):
        try:
            setattr(sky, attr, val)
        except Exception:
            pass
    nt.links.new(sky.outputs[0], bg.inputs[0])
    nt.links.new(bg.outputs[0], out.inputs[0])
    bg.inputs[1].default_value = 0.3
    if not any(o.type == 'LIGHT' and o.data.type == 'SUN' for o in sc.objects):
        ld = bpy.data.lights.new('CINEMA_sun', 'SUN')
        ld.energy = 4
        ld.angle = math.radians(1.5)
        so = bpy.data.objects.new('CINEMA_sun', ld)
        C.objects.link(so)
        so.rotation_euler = (math.radians(60), 0, math.radians(140))


"""Tyre contact patch (runs after drive_scene.py).
The tyres are the .tyre physics size within ~1 cm (mesh 0.346/0.337 m vs Radius 0.335/0.325), yet the
game's tyres squash under load, so on grounded frames the rear sat up to ~5 cm into the visual ground.
Instead of sinking, the tyre's bottom is flattened onto the ground: a Geometry Nodes modifier on each
wheel mesh pushes tyre-material points that are below a per-wheel contact plane back onto it. The plane
is keyed per frame at the ground under the axle, only when the tyre would penetrate, and at most 6 cm up."""
import bpy, math
from mathutils import Vector

sc = bpy.context.scene
O = bpy.data.objects
MAX_SQUASH = 0.06


def ground_z(x, y, top):
    dg = bpy.context.evaluated_depsgraph_get()
    org = Vector((x, y, top))
    for _ in range(30):
        hit, loc, nrm, idx, ob, mat = sc.ray_cast(dg, org, Vector((0, 0, -1)))
        if not hit:
            return None
        if ob and any(c.name == 'track' for c in ob.users_collection):
            return loc.z
        org = loc - Vector((0, 0, 0.01))
    return None


def node_group(plane, mat, tag):
    name = 'CINEMA_tyre_contact_' + tag
    ng = bpy.data.node_groups.get(name)
    if ng:
        bpy.data.node_groups.remove(ng)
    ng = bpy.data.node_groups.new(name, 'GeometryNodeTree')
    ng.is_modifier = True
    ng.interface.new_socket('Geometry', in_out='INPUT', socket_type='NodeSocketGeometry')
    ng.interface.new_socket('Geometry', in_out='OUTPUT', socket_type='NodeSocketGeometry')
    N, L = ng.nodes, ng.links
    gi = N.new('NodeGroupInput')
    go = N.new('NodeGroupOutput')
    oi = N.new('GeometryNodeObjectInfo')
    oi.transform_space = 'RELATIVE'
    rv = N.new('FunctionNodeRotateVector')
    rv.inputs['Vector'].default_value = (0, 0, 1)
    pos = N.new('GeometryNodeInputPosition')
    sub = N.new('ShaderNodeVectorMath'); sub.operation = 'SUBTRACT'
    dot = N.new('ShaderNodeVectorMath'); dot.operation = 'DOT_PRODUCT'
    mn = N.new('ShaderNodeMath'); mn.operation = 'MINIMUM'; mn.inputs[1].default_value = 0.0
    neg = N.new('ShaderNodeMath'); neg.operation = 'MULTIPLY'; neg.inputs[1].default_value = -1.0
    sc_ = N.new('ShaderNodeVectorMath'); sc_.operation = 'SCALE'
    ms = N.new('GeometryNodeMaterialSelection')
    sp = N.new('GeometryNodeSetPosition')
    oi.inputs['Object'].default_value = plane
    L.new(oi.outputs['Rotation'], rv.inputs['Rotation'])
    L.new(pos.outputs['Position'], sub.inputs[0])
    L.new(oi.outputs['Location'], sub.inputs[1])
    L.new(sub.outputs['Vector'], dot.inputs[0])
    L.new(rv.outputs['Vector'], dot.inputs[1])
    L.new(dot.outputs['Value'], mn.inputs[0])
    L.new(mn.outputs['Value'], neg.inputs[0])
    L.new(rv.outputs['Vector'], sc_.inputs[0])
    L.new(neg.outputs['Value'], sc_.inputs['Scale'])
    ms.inputs['Material'].default_value = mat
    L.new(gi.outputs['Geometry'], sp.inputs['Geometry'])
    L.new(ms.outputs['Selection'], sp.inputs['Selection'])
    L.new(sc_.outputs['Vector'], sp.inputs['Offset'])
    L.new(sp.outputs['Geometry'], go.inputs['Geometry'])
    return ng


done = []
for piv in ('fwheel', 'rwheel'):
    # the tyre: a material scene.py marked as rubber (the tyre mod's own sheets), on a mesh that turns with this wheel
    mesh, tyre = None, None
    for o in O[piv].children_recursive:
        if o.type != 'MESH' or o.hide_render:
            continue
        tm = next((s.material for s in o.material_slots if s.material and s.material.get('cinema_tyre')), None)
        if tm is not None:
            mesh, tyre = o, tm
            break
    if mesh is None:
        continue
    tyre_mat = tyre.name
    inv = O[piv].matrix_world.inverted()
    # tyre radius off the mesh, about the axle (local X)
    r = 0.0
    for v in mesh.data.vertices:
        p = inv @ (mesh.matrix_world @ v.co)
        r = max(r, math.hypot(p.y, p.z))
    pl = O.get(f'CINEMA_contact_{piv}') or bpy.data.objects.new(f'CINEMA_contact_{piv}', None)
    if not pl.users_collection:
        sc.collection.objects.link(pl)
    pl.hide_render = True
    if pl.animation_data:
        pl.animation_data_clear()
    md = mesh.modifiers.get('tyre_contact') or mesh.modifiers.new('tyre_contact', 'NODES')
    md.node_group = node_group(pl, bpy.data.materials[tyre_mat], piv)
    squashed = 0
    for fr in range(sc.frame_start, sc.frame_end + 1):
        sc.frame_set(fr)
        c = O[piv].matrix_world.translation
        g = ground_z(c.x, c.y, c.z + 0.5)
        bottom = c.z - r
        if g is not None and g > bottom:
            z = min(g, bottom + MAX_SQUASH)
            squashed += 1
        else:
            z = bottom - 1.0
        pl.location = (c.x, c.y, z)
        pl.keyframe_insert('location', frame=fr)
    done.append((piv, mesh.name, round(r, 3), squashed))
sc.frame_set(sc.frame_start)
print('CONTACT', done)

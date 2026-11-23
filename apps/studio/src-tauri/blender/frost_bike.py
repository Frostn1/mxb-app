# SPDX-License-Identifier: GPL-3.0-or-later
#
# Frost Studio's bike builder, the Blender half.
#
# Copyright (C) 2026 Creste LLC
#
# This program is free software: you can redistribute it and/or modify it under the terms of
# the GNU General Public License as published by the Free Software Foundation, either version
# 3 of the License, or (at your option) any later version. It is distributed in the hope that
# it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See <https://www.gnu.org/licenses/>.
#
# It imports `bpy`, so it is licensed as Blender asks of scripts that do. Studio never links
# it or Blender: it writes this file next to a job and runs the rider's own Blender on it,
#
#     blender -b --factory-startup --python-exit-code 1 --python frost_bike.py -- job.json
#
# and reads back the JSON this writes to job["result"]. Every op answers there, errors
# included, so Studio never has to pick an answer out of Blender's own console output.

import json
import math
import os
import sys
import traceback
from contextlib import contextmanager

import bpy
from mathutils import Euler, Matrix, Vector

# frost_make.py (placeholder parts, Part Maker templates) is written beside this file.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import frost_kit  # noqa: E402
import frost_make  # noqa: E402

MESH_EXTS = {".blend", ".fbx", ".obj"}


def job_path():
    argv = sys.argv
    if "--" not in argv or argv.index("--") + 1 >= len(argv):
        raise SystemExit("usage: blender -b --python frost_bike.py -- job.json")
    return argv[argv.index("--") + 1]


def empty_scene():
    """No default cube, camera or light: only what the job puts there."""
    bpy.ops.wm.read_factory_settings(use_empty=True)


def _tris(obj):
    if obj.type != "MESH":
        return 0
    obj.data.calc_loop_triangles()
    return len(obj.data.loop_triangles)


def import_part(path, report=None):
    """Bring a part file's objects into the scene. Returns the new objects.

    A file with LODs (a Frost Blend's game file: `<page>_LOD0/_LOD1/_LOD2/_SHADOW`
    collections, or the kit's "b"/"c"/"_shadow" name suffixes) brings its LOD0 only: the other
    levels are the same part again, and stacked together they'd be one part at twice its
    weight. `report`, a dict when given, gets frost_kit.pick_lod0's answer under "lod" (None
    for a file without LODs)."""
    ext = os.path.splitext(path)[1].lower()
    if ext not in MESH_EXTS:
        raise ValueError("a part must be .blend, .fbx or .obj, not %s" % (ext or "a bare file"))
    before = set(bpy.data.objects)
    colls_before = set(bpy.data.collections)
    if ext == ".blend":
        # Appended, not linked: the build owns its copy and the rider's file is never touched.
        # Its collections come along only to say which LOD each object is in; they're dropped
        # again below, and the objects go straight into the scene as before.
        with bpy.data.libraries.load(path, link=False) as (src, dst):
            dst.objects = list(src.objects)
            dst.collections = list(src.collections)
    elif ext == ".fbx":
        bpy.ops.import_scene.fbx(filepath=path)
    else:
        bpy.ops.wm.obj_import(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    keep, lod = frost_kit.pick_lod0(
        [{"name": o.name, "collections": [c.name for c in o.users_collection], "tris": _tris(o)} for o in new]
    )
    for o in new:
        if o.name not in keep:
            bpy.data.objects.remove(o)
    objs = [o for o in bpy.data.objects if o not in before]
    if ext == ".blend":
        scene_objects = bpy.context.scene.collection.objects
        for obj in objs:
            if obj.name not in scene_objects:
                scene_objects.link(obj)
        for c in [c for c in bpy.data.collections if c not in colls_before]:
            bpy.data.collections.remove(c)
    if report is not None:
        report["lod"] = lod
    # Appended objects carry their local transforms but not yet their world ones: without
    # this every matrix_world reads as the origin, and so would every bound and empty.
    bpy.context.view_layer.update()
    return objs


def world_bounds(objs):
    lo = [math.inf] * 3
    hi = [-math.inf] * 3
    for obj in objs:
        for corner in obj.bound_box:
            w = obj.matrix_world @ Vector(corner)
            for i in range(3):
                lo[i] = min(lo[i], w[i])
                hi[i] = max(hi[i], w[i])
    if lo[0] == math.inf:
        return None
    return {"min": lo, "max": hi}


def describe(obj):
    info = {
        "name": obj.name,
        "type": obj.type,
        "parent": obj.parent.name if obj.parent else None,
        "location": list(obj.matrix_world.translation),
        "missingTextures": [],
    }
    if obj.get("mxb_node"):
        # The game node a Frost Blend's kit parents this to (steer, fsusp ...).
        info["node"] = str(obj.get("mxb_node"))
    if obj.type == "MESH":
        mesh = obj.data
        mesh.calc_loop_triangles()
        info["verts"] = len(mesh.vertices)
        info["tris"] = len(mesh.loop_triangles)
        info["materials"] = [m.name for m in mesh.materials if m is not None]
        info["uv"] = len(mesh.uv_layers) > 0
        missing_textures = []
        for material in mesh.materials:
            if not (material and material.use_nodes and material.node_tree):
                continue
            for node in material.node_tree.nodes:
                if node.type != "TEX_IMAGE" or node.image is None:
                    continue
                image = node.image
                if image.source == "FILE" and image.packed_file is None:
                    if not os.path.exists(bpy.path.abspath(image.filepath)):
                        missing_textures.append(bpy.path.basename(image.filepath) if image.filepath else image.name)
        info["missingTextures"] = sorted(set(missing_textures))
    return info


def _layer_paths(lc, target, path=()):
    """Layer-collection chains from `lc` down to every layer collection of `target`."""
    path = path + (lc,)
    if lc.collection == target:
        yield path
    for c in lc.children:
        yield from _layer_paths(c, target, path)


@contextmanager
def visible_for_export(objects):
    """Blender exports nothing it can't select: an object in an excluded or hidden collection,
    or hidden itself, would leave the FBX without a word (an exhaust kept in its own hidden
    collection). Show them for the export and put every flag back after."""
    saved = []
    hidden = []

    def force(thing, attr, value):
        if getattr(thing, attr) != value:
            saved.append((thing, attr, getattr(thing, attr)))
            setattr(thing, attr, value)

    try:
        root = bpy.context.view_layer.layer_collection
        for coll in {c for o in objects for c in o.users_collection}:
            for chain in _layer_paths(root, coll):
                for lc in chain[1:]:
                    force(lc, "exclude", False)
                    force(lc, "hide_viewport", False)
                    force(lc.collection, "hide_viewport", False)
                    force(lc.collection, "hide_select", False)
        for o in objects:
            force(o, "hide_viewport", False)
            force(o, "hide_select", False)
            if o.hide_get():
                hidden.append(o)
                o.hide_set(False)
        yield
    finally:
        for o in hidden:
            o.hide_set(True)
        for thing, attr, value in reversed(saved):
            setattr(thing, attr, value)


# Where game_textures_for_export wires each map for the FBX exporter: Base Color is the FBX's
# DiffuseColor, Roughness its ShininessExponent, which the converter reads as the spec map.
_GAME_INPUTS = (("colour", "Base Color"), ("spec", "Roughness"))


def _image_file_for_export(img, tex_dir, taken, undo):
    """An image the FBX can point at: its own file when that's on disk, else its packed bytes
    written once to `tex_dir` (a Frost Blend packs its textures, and their recorded paths are
    the seller's own folders)."""
    path = bpy.path.abspath(img.filepath) if img.filepath else ""
    if (path and os.path.isfile(path)) or img.packed_file is None:
        return
    base = bpy.path.basename(img.filepath) or img.name
    stem, ext = os.path.splitext(base)
    if not ext:
        ext = "." + (img.file_format or "PNG").lower().replace("targa", "tga").replace("jpeg", "jpg")
    stem = frost_kit.safe_tag(stem, "texture")
    name, n = stem + ext, 1
    while name.lower() in taken:
        n += 1
        name = "%s-%d%s" % (stem, n, ext)
    taken.add(name.lower())
    os.makedirs(tex_dir, exist_ok=True)
    dst = os.path.join(tex_dir, name)
    with open(dst, "wb") as f:
        f.write(img.packed_file.data)
    undo.append(lambda img=img, old=img.filepath_raw: setattr(img, "filepath_raw", old))
    img.filepath_raw = dst


@contextmanager
def game_textures_for_export(objects, tex_dir):
    """Each material's colour, spec and normal maps where the FBX exporter finds them, for the
    export only. The exporter follows an image wired straight into the shader and writes the
    path it was loaded from, so a Frost Blend's view material (the colour sheet through a Mix,
    the spec map through Map Range, all of it packed) came out with no colour texture and the
    converter made the part grey. frost_kit.game_maps says which image is which; it's wired
    straight in, its packed bytes written out, and every link and path put back after."""
    undo = []
    taken = set()
    try:
        mats = {m for o in objects if o.type == "MESH" for m in o.data.materials if m and m.use_nodes and m.node_tree}
        for m in sorted(mats, key=lambda m: m.name):
            nt = m.node_tree
            bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
            if bsdf is None:
                continue
            images = {n.name for n in nt.nodes if n.type == "TEX_IMAGE" and n.image is not None}
            if not images:
                continue
            links = [(l.from_node.name, l.to_node.name, l.to_socket.name) for l in nt.links]
            maps = frost_kit.game_maps(links, images, bsdf.name)
            for role, socket in _GAME_INPUTS:
                inp = bsdf.inputs.get(socket)
                if maps[role] is None or inp is None:
                    continue
                out = nt.nodes[maps[role]].outputs["Color"]
                old = [l.from_socket for l in inp.links]
                if old == [out]:
                    continue
                for l in list(inp.links):
                    nt.links.remove(l)
                nt.links.new(out, inp)

                def put_back(nt=nt, inp=inp, old=old):
                    for l in list(inp.links):
                        nt.links.remove(l)
                    for s in old:
                        nt.links.new(s, inp)

                undo.append(put_back)
            for name in [n for n in maps.values() if n]:
                _image_file_for_export(nt.nodes[name].image, tex_dir, taken, undo)
        yield
    finally:
        for step in reversed(undo):
            step()


def export_fbx(path, objs=None):
    """`objs`, when given, exports only those objects — `op_split` uses this to give each
    group its own standalone file, which is what a later build imports for that group
    instead of pulling in the whole bike it was cut from. Left `None` (every other caller),
    this is the same whole-scene export it always was."""
    targets = list(objs) if objs is not None else list(bpy.context.scene.objects)
    tex_dir = os.path.join(os.path.dirname(os.path.abspath(path)), "textures")
    with visible_for_export(targets), game_textures_for_export(targets, tex_dir):
        if objs is not None:
            bpy.ops.object.select_all(action="DESELECT")
            for o in objs:
                o.select_set(True)
        # Pinned so every build exports the same way. Axes and scale are checked against the
        # reference bike in the converter before anything is trusted in the game. "OTHER" is
        # curves, text and surfaces, which the exporter turns into meshes; left out, a curve
        # exhaust would leave the FBX.
        bpy.ops.export_scene.fbx(
            filepath=path,
            use_selection=objs is not None,
            object_types={"MESH", "EMPTY", "OTHER"},
            apply_unit_scale=True,
            apply_scale_options="FBX_SCALE_ALL",
            axis_forward="-Z",
            axis_up="Y",
            mesh_smooth_type="FACE",
            add_leaf_bones=False,
            bake_anim=False,
            # A kit part's `mxb_node` (which game node it's modelled in) has to survive a split
            # group's own FBX, or refreshing that group would forget where it goes.
            use_custom_props=True,
        )


def export_glb(path, objs=None):
    """What Studio's preview draws. `objs`, when given, exports only those objects — the
    whole-bike split (`op_split`) needs one group's geometry per file, not the scene the
    other groups are still sitting in. Left `None` (every other caller), this is the same
    "everything in the scene" export it always was: the scene never holds anything but the
    one part being catalogued in the first place."""
    if objs is not None:
        bpy.ops.object.select_all(action="DESELECT")
        for o in objs:
            o.select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=path, export_format="GLB", use_selection=objs is not None, export_apply=True
    )


def op_inspect(job):
    """Phase A's round trip: import one part, say what's in it, export FBX and a preview."""
    empty_scene()
    objs = import_part(job["part"])
    if not objs:
        raise ValueError("nothing to import in %s" % os.path.basename(job["part"]))
    out = {
        "objects": [describe(o) for o in objs],
        "bounds": world_bounds([o for o in objs if o.type == "MESH"]),
        "tris": sum(len(o.data.loop_triangles) for o in objs if o.type == "MESH"),
    }
    if job.get("fbx"):
        export_fbx(job["fbx"])
        out["fbx"] = job["fbx"]
    if job.get("glb"):
        export_glb(job["glb"])
        out["glb"] = job["glb"]
    return out


def empties(objs):
    """The attach points a part brings: every empty, where it sits in the world and which way
    it's turned (its world rotation, scale taken out, as three rows). A bike exported from the
    game carries one empty per game node (`steer`, `fsusp` ...) turned the way the node is:
    Studio snaps a kit part, modelled in that node's own space, onto it."""
    out = []
    for o in objs:
        if o.type != "EMPTY":
            continue
        rot = o.matrix_world.to_3x3().normalized()
        out.append({
            "name": o.name,
            "parent": o.parent.name if o.parent else None,
            "location": list(o.matrix_world.translation),
            "rotation": [list(row) for row in rot],
        })
    return out


def render_thumb(path, meshes, size):
    """A small picture of the part, from the front-right and a little above, on transparent.

    Workbench, not Eevee or Cycles: it needs no lights or materials, draws in a second and
    shows the shape, which is all a tray thumbnail is for.
    """
    bounds = world_bounds(meshes)
    if bounds is None:
        return False
    lo, hi = Vector(bounds["min"]), Vector(bounds["max"])
    centre = (lo + hi) / 2
    radius = max((hi - lo).length / 2, 1e-3)

    scene = bpy.context.scene
    cam_data = bpy.data.cameras.new("frost_thumb")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = radius * 2.2
    cam_data.clip_end = radius * 20
    cam = bpy.data.objects.new("frost_thumb", cam_data)
    scene.collection.objects.link(cam)
    # Blender is Z up; a bike part faces -Y, so look from the front-right, above.
    direction = Vector((1.0, -1.2, 0.7)).normalized()
    cam.location = centre + direction * radius * 6
    cam.rotation_euler = (-direction).to_track_quat("-Z", "Y").to_euler()
    scene.camera = cam

    scene.render.engine = "BLENDER_WORKBENCH"
    scene.render.resolution_x = size
    scene.render.resolution_y = size
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    shading = scene.display.shading
    shading.light = "STUDIO"
    shading.color_type = "MATERIAL"
    shading.show_cavity = True
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    # The camera was only for the picture: nothing exported after this should carry it.
    bpy.data.objects.remove(cam)
    bpy.data.cameras.remove(cam_data)
    return os.path.isfile(path)


def catalog(objs, job):
    """What the part library keeps of `objs`: their descriptions, attach empties, a
    thumbnail, and a GLB for the preview. A thumbnail that won't render is a part without a
    picture, never a part that can't be added.

    `objs` scopes the GLB to that group alone — for a plain add this is every object in the
    scene (the same as exporting the whole thing), but `op_split` calls this once per group
    while the others are still sitting in the same scene, and each group's file must hold
    only its own geometry."""
    meshes = [o for o in objs if o.type == "MESH"]
    out = {
        "objects": [describe(o) for o in objs],
        "empties": empties(objs),
        "bounds": world_bounds(meshes),
        "tris": sum(len(o.data.loop_triangles) for o in meshes),
        # What "Split into parts" would find, so the tray can say so before anyone tries:
        # "node" / "material" / "loose", or None when the part is one piece whichever way.
        "splitBy": split_way(objs),
    }
    if job.get("thumb"):
        # Rendering, not exporting, so `use_selection` has no say here — for a plain add the
        # scene never holds anything but `objs` anyway, but `op_split` leaves every other
        # group sitting right there, and a thumbnail with the rest of the bike still in
        # frame isn't a picture of the part. Hidden from the render only, not deleted, and
        # put back whether the render worked or not.
        rest = [o for o in bpy.data.objects if o not in set(objs) and not o.hide_render]
        for o in rest:
            o.hide_render = True
        try:
            if render_thumb(job["thumb"], meshes, int(job.get("thumbSize") or 256)):
                out["thumb"] = job["thumb"]
        except Exception as e:
            out["thumbError"] = "%s: %s" % (type(e).__name__, e)
        finally:
            for o in rest:
                o.hide_render = False
    if job.get("glb"):
        export_glb(job["glb"], objs)
        out["glb"] = job["glb"]
    return out


def op_catalog(job):
    """Phase B: one part file, catalogued for the library.

    A Frost Blend's game file whose LOD0 holds more than one game node (a fork: `steer` and
    `fsusp`) is split straight away when the job gives a `partsDir`, the way "Split into parts"
    would: the answer is then `{"groups": [...]}` as `op_split` gives, one part per node."""
    empty_scene()
    report = {}
    objs = import_part(job["part"], report)
    if not objs:
        raise ValueError("nothing to import in %s" % os.path.basename(job["part"]))
    lod = report.get("lod")
    if lod is not None and job.get("partsDir"):
        buckets = node_buckets(objs)
        if len(buckets) >= 2 and "unassigned" not in buckets:
            return {"groups": export_groups(buckets, job, lod), "lod": lod}
    out = catalog(objs, job)
    out["lod"] = lod
    return out


def node_buckets(objs):
    """`objs` grouped as frost_kit.group_objects says, "unassigned" for those it can't place."""
    groups = frost_kit.group_objects([
        {"name": o.name, "parent": o.parent.name if o.parent else None,
         "node": str(o.get("mxb_node")) if o.get("mxb_node") else None}
        for o in objs
    ])
    buckets = {}
    for o in objs:
        buckets.setdefault(groups[o.name] or "unassigned", []).append(o)
    return buckets


def export_groups(buckets, job, lod, roles=True):
    """Each bucket as a part of its own: a standalone FBX in `job["partsDir"]` (a folder that
    outlives this job, unlike `job["workDir"]`), plus what `catalog` keeps of it.

    `roles`: the bucket keys are roles (a split by node or name). A split along the geometry
    (by material or loose piece) keys them by piece name instead, with no role."""
    work = job["workDir"]
    parts_dir = job["partsDir"]
    os.makedirs(parts_dir, exist_ok=True)
    groups = []
    # Sorted so the result (and so the tag on each exported file) is the same every run,
    # not whatever order Blender happened to import the objects in.
    for i, (key, group_objs) in enumerate(sorted(buckets.items())):
        if roles:
            tag = key if key != "unassigned" else "part-%d" % (i + 1)
        else:
            tag = key
        source = os.path.join(parts_dir, "%s.fbx" % tag)
        export_fbx(source, group_objs)
        sub = catalog(
            group_objs,
            {"thumb": os.path.join(work, "%s-thumb.png" % tag), "thumbSize": job.get("thumbSize") or 256,
             "glb": os.path.join(work, "%s.glb" % tag)},
        )
        sub["tag"] = tag
        sub["role"] = None if (key == "unassigned" or not roles) else key
        sub["source"] = source
        sub["lod"] = lod
        groups.append(sub)
    return groups


def used_materials(objs):
    """The material names the faces of `objs` actually use (an empty slot, or one no face
    points at, isn't a piece)."""
    names = set()
    for o in objs:
        if o.type != "MESH":
            continue
        mats = o.data.materials
        for i in {p.material_index for p in o.data.polygons}:
            m = mats[i] if i < len(mats) else None
            names.add(m.name if m is not None else "")
    return names


def loose_count(objs, stop=2):
    """How many separate shells the meshes of `objs` hold, counting no further than `stop`."""
    import bmesh

    total = 0
    for o in objs:
        if o.type != "MESH" or not o.data.polygons:
            continue
        bm = bmesh.new()
        try:
            bm.from_mesh(o.data)
            seen = set()
            for start in bm.verts:
                if start.index in seen:
                    continue
                total += 1
                if total >= stop:
                    return total
                stack = [start]
                seen.add(start.index)
                while stack:
                    v = stack.pop()
                    for e in v.link_edges:
                        w = e.other_vert(v)
                        if w.index not in seen:
                            seen.add(w.index)
                            stack.append(w)
        finally:
            bm.free()
    return total


def split_way(objs, by="auto"):
    """How a split would cut `objs`: "node" (kit nodes or role names), "material", "loose", or
    None when every way gives one piece. `by` asks for one way; "auto" tries them in order."""
    if by in ("auto", "node") and len(node_buckets(objs)) >= 2:
        return "node"
    if by in ("auto", "material") and len(used_materials(objs)) >= 2:
        return "material"
    if by in ("auto", "loose") and loose_count(objs) >= 2:
        return "loose"
    return None


def separate(objs, how):
    """Cut every mesh in `objs` by `how` ("MATERIAL" or "LOOSE"), in place: Blender's own
    Separate. The new objects keep the source's transform, parent and modifiers."""
    meshes = [o for o in objs if o.type == "MESH" and o.data.polygons]
    others = [o for o in objs if o not in meshes]
    out = []
    for o in meshes:
        before = set(bpy.data.objects)
        if bpy.context.object and bpy.context.object.mode != "OBJECT":
            bpy.ops.object.mode_set(mode="OBJECT")
        bpy.ops.object.select_all(action="DESELECT")
        o.select_set(True)
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        try:
            bpy.ops.mesh.separate(type=how)
        finally:
            bpy.ops.object.mode_set(mode="OBJECT")
        out.append(o)
        out.extend(x for x in bpy.data.objects if x not in before)
    return out, others


def piece_name(o, fallback):
    """A piece's name: its material's, else the object's."""
    if o.type == "MESH":
        for m in o.data.materials:
            if m is not None:
                return m.name
    return fallback


def geometry_buckets(objs, how):
    """`objs` cut by material or loose piece, as {tag: [objects]}, tags named from materials.
    Objects that aren't meshes (empties, armatures) ride along with the largest piece."""
    pieces, others = separate(objs, "MATERIAL" if how == "material" else "LOOSE")
    pieces = [o for o in pieces if o.type == "MESH" and o.data.polygons]
    for o in pieces:
        o.data.calc_loop_triangles()
    if how == "material":
        by_name = {}
        for o in pieces:
            by_name.setdefault(piece_name(o, o.name), []).append(o)
        names = sorted(by_name, key=lambda n: -sum(len(o.data.loop_triangles) for o in by_name[n]))
        groups = [by_name[n] for n in names]
    else:
        names = [piece_name(o, o.name) for o in pieces]
        groups = [[o] for o in pieces]
    sizes = [(names[i], sum(len(o.data.loop_triangles) for o in groups[i])) for i in range(len(groups))]
    own, rest = frost_kit.keep_largest(sizes)
    kept_names = [names[i] for i in own] + (["%s-rest" % names[rest[0]]] if rest else [])
    kept = [groups[i] for i in own] + ([[o for i in rest for o in groups[i]]] if rest else [])
    tags = frost_kit.unique_tags(kept_names)
    buckets = dict(zip(tags, kept))
    if others and buckets:
        buckets[tags[0]].extend(others)
    return buckets


def op_split(job):
    """A full bike (or a big sub-assembly) brought in as one file, cut into its parts by
    name — each object's kit node (`mxb_node`) or own name if it hints at a role, else its
    nearest named ancestor's. Anything left with no hint anywhere up its chain is its own
    "unassigned" group, never silently dropped or folded into whichever group happened to
    import first.

    Each group gets its own standalone FBX, in `job["partsDir"]` — not just a GLB and a
    thumbnail. A later build imports a part's `source` fresh (`op_assemble` doesn't work from
    the catalogued GLB), and that has to be the group's own file, not the whole bike it was
    cut from, or building with a split part in a slot would import the entire bike into every
    slot that used one. The caller (`bike_part_split` in `main.rs`) turns each group into its
    own library part, with this FBX as its `source`, and removes the one part this replaces."""
    empty_scene()
    report = {}
    objs = import_part(job["part"], report)
    if not objs:
        raise ValueError("nothing to import in %s" % os.path.basename(job["part"]))
    lod = report.get("lod")
    # `by`: "auto" (default) splits by kit node or role name, else by material, else by loose
    # piece; or one of "node" / "material" / "loose" to ask for that way only. A file that is
    # one piece whichever way is an answer, not an error: `{"groups": [], "nothing": true}`,
    # which Studio turns into its own `nothing_to_split` code rather than a Python traceback.
    way = split_way(objs, job.get("by") or "auto")
    if way is None:
        return {"groups": [], "nothing": True}
    if way == "node":
        return {"groups": export_groups(node_buckets(objs), job, lod), "by": way}
    buckets = geometry_buckets(objs, way)
    if len(buckets) < 2:
        return {"groups": [], "nothing": True}
    return {"groups": export_groups(buckets, job, lod, roles=False), "by": way}


def save_part(path):
    """The scene as a part file of its own, which the library then treats like any other."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=path, copy=True, check_existing=False)


def op_placeholder(job):
    """Studio's placeholder bike, one .blend per role, each catalogued as it's made: so the
    whole builder can be tried before anyone brings parts. One Blender run for all eight."""
    parts = []
    for role in frost_make.PLACEHOLDER_ROLES:
        empty_scene()
        frost_make.placeholder(role)
        bpy.context.view_layer.update()
        path = os.path.join(job["dir"], "placeholder_%s.blend" % role)
        save_part(path)
        work = os.path.join(job["work"], role)
        os.makedirs(work, exist_ok=True)
        answer = catalog(list(bpy.context.scene.objects), {
            "thumb": os.path.join(work, "thumb.png"),
            "thumbSize": job.get("thumbSize"),
            "glb": os.path.join(work, "part.glb"),
        })
        answer["role"] = role
        answer["part"] = path
        parts.append(answer)
    return {"parts": parts}


def op_templates(job):
    return {"templates": frost_make.describe_templates()}


# What model-written code may use. Checked before it runs, as well as run with only these
# names: a part is geometry, so nothing here reaches the file system, the network or Blender's
# own settings.
SAFE_MODULES = {"bpy", "bmesh", "math", "mathutils", "random"}
SAFE_BUILTINS = {
    "abs", "all", "any", "bool", "dict", "enumerate", "float", "int", "isinstance", "len",
    "list", "max", "min", "print", "range", "reversed", "round", "set", "sorted", "str",
    "sum", "tuple", "zip", "ValueError", "Exception", "True", "False", "None",
}
BANNED_NAMES = {
    "eval", "exec", "compile", "open", "input", "globals", "locals", "vars", "getattr",
    "setattr", "delattr", "breakpoint", "help", "memoryview", "type", "object", "super",
}
# Attribute chains no part needs: files, scripts, preferences, add-ons, handlers, drivers.
BANNED_ATTRS = {
    "wm", "script", "preferences", "utils", "app", "libraries", "texts", "filepath",
    "export_scene", "import_scene", "import_mesh", "export_mesh", "file", "image", "images",
    "driver_add", "driver_namespace", "drivers", "handlers", "addon_utils", "render",
    "sequencer", "console", "text", "screen", "window", "window_manager", "context_pointer_set",
    "gi_frame", "f_globals", "f_builtins", "cr_frame", "tb_frame",
}


def check_code(src):
    """Refuse code that steps outside making geometry, before any of it runs."""
    import ast

    tree = ast.parse(src, mode="exec")
    for node in ast.walk(tree):
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            mods = [a.name for a in node.names] if isinstance(node, ast.Import) else [node.module or ""]
            for m in mods:
                if m.split(".")[0] not in SAFE_MODULES:
                    raise ValueError("the code imports %s, which a part doesn't need" % m)
            # `from random import __builtins__ as b` names no banned Name or Attribute node at
            # all: it is the import itself that hands out a live, unrestricted namespace. Every
            # name an import binds, on either side of `as`, is checked the same as a bare name.
            for a in node.names:
                for bound in (a.name, a.asname):
                    if bound and (bound in BANNED_NAMES or bound.startswith("_") or "__" in bound):
                        raise ValueError("the code imports %s, which a part doesn't need" % bound)
        elif isinstance(node, ast.Name) and (node.id in BANNED_NAMES or node.id.startswith("__")):
            raise ValueError("the code uses %s, which a part doesn't need" % node.id)
        elif isinstance(node, ast.Attribute) and (node.attr in BANNED_ATTRS or node.attr.startswith("_")):
            raise ValueError("the code reaches for .%s, which a part doesn't need" % node.attr)
        elif isinstance(node, (ast.Global, ast.Nonlocal, ast.AsyncFunctionDef, ast.Await, ast.Lambda)):
            raise ValueError("the code uses %s, which a part doesn't need" % type(node).__name__)
        elif isinstance(node, ast.Constant) and isinstance(node.value, str) and "__" in node.value:
            raise ValueError("the code carries a dunder string")
    return tree


def run_code(src, role, mount):
    """Run checked code that builds a part. It gets the template helpers, not the file system."""
    check_code(src)
    import builtins
    import importlib
    import bmesh
    import random

    def safe_import(name, globals=None, locals=None, fromlist=(), level=0):
        if level != 0 or name.split(".")[0] not in SAFE_MODULES:
            raise ImportError(name)
        return importlib.__import__(name, globals, locals, fromlist, level)

    safe = {n: getattr(builtins, n) for n in SAFE_BUILTINS if hasattr(builtins, n)}
    safe["__import__"] = safe_import
    env = {
        "__builtins__": safe,
        "bpy": bpy, "bmesh": bmesh, "math": math, "random": random, "Vector": Vector, "Matrix": Matrix,
        "material": frost_make.material, "hex_rgba": frost_make.hex_rgba, "empty": frost_make.empty,
        "tube": frost_make.tube, "box": frost_make.box, "curve_object": frost_make.curve_object,
        "to_mesh": frost_make.to_mesh,
    }
    exec(compile(src, "<part maker>", "exec"), env)
    # Whatever it made hangs from one root with the mount empty the builder snaps by.
    objs = list(bpy.context.scene.objects)
    if not any(o.type == "MESH" for o in objs):
        raise ValueError("the code made no geometry")
    names = {o.name.split(".")[0].lower() for o in objs if o.type == "EMPTY"}
    root = frost_make.empty(role, (0, 0, 0))
    if mount and mount not in names:
        frost_make.empty(mount, (0, 0, 0), root)
    for o in objs:
        if o.parent is None:
            mw = o.matrix_world.copy()
            o.parent = root
            o.matrix_world = mw
    return root


def op_make(job):
    """The Part Maker: build a part from a template and its sliders, or from checked code, save
    it as a .blend, and catalog it for the preview."""
    empty_scene()
    if job.get("code"):
        role = job.get("role") or "handguards"
        run_code(job["code"], role, frost_make.ROLE_MOUNT.get(role))
    else:
        t = frost_make.TEMPLATES.get(job.get("template"))
        if t is None:
            raise ValueError("no template %r" % job.get("template"))
        t["make"](job.get("params") or {})
        role = t["role"]
    bpy.context.view_layer.update()
    save_part(job["blend"])
    out = catalog(list(bpy.context.scene.objects), job)
    out["role"] = role
    out["blend"] = job["blend"]
    return out


def part_turn(p):
    """The rider's turn of one part, about its pivot: identity for a part nobody turned (and
    for a job from before turns existed). Euler XYZ, as Studio's preview and anchors use."""
    rot = p.get("rotation") or [0.0, 0.0, 0.0]
    if not any(rot):
        return Matrix.Identity(4)
    pivot = Vector(p.get("pivot") or [0.0, 0.0, 0.0])
    turn = Euler([math.radians(a) for a in rot], "XYZ").to_matrix().to_4x4()
    return Matrix.Translation(pivot) @ turn @ Matrix.Translation(-pivot)


def part_basis(p):
    """How a part modelled in its game node's own space (a kit part) is turned to sit on the
    bike, before its offset: three rows, or identity for every other part (and for a job
    from before this existed)."""
    basis = p.get("basis")
    if not basis:
        return Matrix.Identity(4)
    return Matrix(basis).to_4x4()


def op_assemble(job):
    """Phase E: the slotted parts, each moved onto its anchor and carried into the frame of
    the game part it's built into, under roots named for those parts. Exported as the model's
    FBX, then again as a low, plain white shadow.

    The roots sit at the origin, each turned about X by job["rootTurns"][group] degrees. The
    converter drops a root's position and writes the part's own axes in place of its
    rotation, as fbx2edf.exe does; a root turned this way is one those axes stand in for
    exactly. The meshes keep their world placement under it, and the template's .geom places
    each part, so all the placing is in the meshes. Empties are dropped; the lever and peg
    objects keep their names, which the game's gfx.cfg animates them by.
    """
    empty_scene()
    turns = job.get("rootTurns") or {}
    roots = {}
    for group in job["groups"]:
        root = bpy.data.objects.new(group, None)
        bpy.context.scene.collection.objects.link(root)
        root.rotation_euler = (math.radians(turns.get(group, 0.0)), 0.0, 0.0)
        roots[group] = root
    # The roots' turns into their world matrices, before any mesh is placed under them.
    bpy.context.view_layer.update()
    for p in job["parts"]:
        objs = import_part(p["source"])
        into = Matrix(job["groups"][p["group"]]) @ part_turn(p) @ Matrix.Translation(Vector(p["offset"])) @ part_basis(p)
        worlds = {o: into @ o.matrix_world for o in objs if o.type == "MESH"}
        for o, w in worlds.items():
            o.parent = roots[p["group"]]
            o.matrix_parent_inverse = Matrix.Identity(4)
            o.matrix_world = w
        for o in objs:
            if o.type != "MESH":
                bpy.data.objects.remove(o)
    bpy.context.view_layer.update()
    for group in [g for g, r in roots.items() if not r.children]:
        bpy.data.objects.remove(roots.pop(group))
    tris = {}
    for group, root in roots.items():
        tris[group] = 0
        for o in root.children:
            o.data.calc_loop_triangles()
            tris[group] += len(o.data.loop_triangles)
    export_fbx(job["fbx"])

    # The shadow: each part's meshes joined and cut down, one white material.
    white = frost_make.material("shadow", (1, 1, 1, 1), roughness=1.0)
    target = int(job.get("shadowTris") or 600)
    shadow_tris = {}
    for group, root in roots.items():
        kids = [o for o in root.children if o.type == "MESH"]
        bpy.ops.object.select_all(action="DESELECT")
        for o in kids:
            o.select_set(True)
        bpy.context.view_layer.objects.active = kids[0]
        if len(kids) > 1:
            bpy.ops.object.join()
        joined = bpy.context.view_layer.objects.active
        joined.name = group + "_shadow"
        joined.data.materials.clear()
        joined.data.materials.append(white)
        joined.data.calc_loop_triangles()
        have = len(joined.data.loop_triangles)
        if have > target:
            mod = joined.modifiers.new("decimate", "DECIMATE")
            mod.ratio = max(0.01, target / have)
            bpy.ops.object.modifier_apply(modifier=mod.name)
        joined.data.calc_loop_triangles()
        shadow_tris[group] = len(joined.data.loop_triangles)
    export_fbx(job["shadowFbx"])
    return {"fbx": job["fbx"], "shadowFbx": job["shadowFbx"], "tris": tris, "shadowTris": shadow_tris}


OPS = {
    "inspect": op_inspect,
    "catalog": op_catalog,
    "split": op_split,
    "placeholder": op_placeholder,
    "templates": op_templates,
    "make": op_make,
    "assemble": op_assemble,
}


def main():
    path = job_path()
    with open(path, encoding="utf-8") as f:
        job = json.load(f)
    result = job.get("result") or os.path.join(os.path.dirname(path), "result.json")
    try:
        op = OPS.get(job.get("op"))
        if op is None:
            raise ValueError("unknown op %r" % job.get("op"))
        answer = op(job)
        answer["blender"] = bpy.app.version_string
    except Exception as e:  # every failure goes back as an answer, not a stack in a log
        answer = {"error": "%s: %s" % (type(e).__name__, e), "trace": traceback.format_exc()}
    with open(result, "w", encoding="utf-8") as f:
        json.dump(answer, f)


main()

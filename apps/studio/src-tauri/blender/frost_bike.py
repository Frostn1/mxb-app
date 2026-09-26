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

import bpy
from mathutils import Vector

MESH_EXTS = {".blend", ".fbx", ".obj"}


def job_path():
    argv = sys.argv
    if "--" not in argv or argv.index("--") + 1 >= len(argv):
        raise SystemExit("usage: blender -b --python frost_bike.py -- job.json")
    return argv[argv.index("--") + 1]


def empty_scene():
    """No default cube, camera or light: only what the job puts there."""
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_part(path):
    """Bring every object in a part file into the scene. Returns the new objects."""
    ext = os.path.splitext(path)[1].lower()
    if ext not in MESH_EXTS:
        raise ValueError("a part must be .blend, .fbx or .obj, not %s" % (ext or "a bare file"))
    before = set(bpy.data.objects)
    if ext == ".blend":
        # Appended, not linked: the build owns its copy and the rider's file is never touched.
        with bpy.data.libraries.load(path, link=False) as (src, dst):
            dst.objects = list(src.objects)
        for obj in dst.objects:
            if obj is not None and obj.name not in bpy.context.scene.collection.objects:
                bpy.context.scene.collection.objects.link(obj)
    elif ext == ".fbx":
        bpy.ops.import_scene.fbx(filepath=path)
    else:
        bpy.ops.wm.obj_import(filepath=path)
    # Appended objects carry their local transforms but not yet their world ones: without
    # this every matrix_world reads as the origin, and so would every bound and empty.
    bpy.context.view_layer.update()
    return [o for o in bpy.data.objects if o not in before]


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
    }
    if obj.type == "MESH":
        mesh = obj.data
        mesh.calc_loop_triangles()
        info["verts"] = len(mesh.vertices)
        info["tris"] = len(mesh.loop_triangles)
        info["materials"] = [m.name for m in mesh.materials if m is not None]
        info["uv"] = len(mesh.uv_layers) > 0
    return info


def export_fbx(path):
    # Pinned so every build exports the same way. Axes and scale are checked against the
    # reference bike in the converter before anything is trusted in the game.
    bpy.ops.export_scene.fbx(
        filepath=path,
        use_selection=False,
        object_types={"MESH", "EMPTY"},
        apply_unit_scale=True,
        apply_scale_options="FBX_SCALE_ALL",
        axis_forward="-Z",
        axis_up="Y",
        mesh_smooth_type="FACE",
        add_leaf_bones=False,
        bake_anim=False,
    )


def export_glb(path):
    """What Studio's preview draws."""
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=False, export_apply=True)


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
    """The attach points a part brings: every empty, where it sits in the world."""
    return [
        {"name": o.name, "parent": o.parent.name if o.parent else None, "location": list(o.matrix_world.translation)}
        for o in objs
        if o.type == "EMPTY"
    ]


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


def op_catalog(job):
    """Phase B: what the part library keeps of a part. Its objects, its attach empties, a
    thumbnail, and a GLB for the preview. A thumbnail that won't render is a part without
    a picture, never a part that can't be added."""
    empty_scene()
    objs = import_part(job["part"])
    if not objs:
        raise ValueError("nothing to import in %s" % os.path.basename(job["part"]))
    meshes = [o for o in objs if o.type == "MESH"]
    out = {
        "objects": [describe(o) for o in objs],
        "empties": empties(objs),
        "bounds": world_bounds(meshes),
        "tris": sum(len(o.data.loop_triangles) for o in meshes),
    }
    if job.get("thumb"):
        try:
            if render_thumb(job["thumb"], meshes, int(job.get("thumbSize") or 256)):
                out["thumb"] = job["thumb"]
        except Exception as e:
            out["thumbError"] = "%s: %s" % (type(e).__name__, e)
    if job.get("glb"):
        export_glb(job["glb"])
        out["glb"] = job["glb"]
    return out


OPS = {"inspect": op_inspect, "catalog": op_catalog}


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

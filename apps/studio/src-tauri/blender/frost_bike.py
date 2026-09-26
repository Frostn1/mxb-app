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
from mathutils import Matrix, Vector

# frost_make.py (placeholder parts, Part Maker templates) is written beside this file.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
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


def export_fbx(path, objs=None):
    """`objs`, when given, exports only those objects — `op_split` uses this to give each
    group its own standalone file, which is what a later build imports for that group
    instead of pulling in the whole bike it was cut from. Left `None` (every other caller),
    this is the same whole-scene export it always was."""
    if objs is not None:
        bpy.ops.object.select_all(action="DESELECT")
        for o in objs:
            o.select_set(True)
    # Pinned so every build exports the same way. Axes and scale are checked against the
    # reference bike in the converter before anything is trusted in the game.
    bpy.ops.export_scene.fbx(
        filepath=path,
        use_selection=objs is not None,
        object_types={"MESH", "EMPTY"},
        apply_unit_scale=True,
        apply_scale_options="FBX_SCALE_ALL",
        axis_forward="-Z",
        axis_up="Y",
        mesh_smooth_type="FACE",
        add_leaf_bones=False,
        bake_anim=False,
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
    """Phase B: one part file, catalogued for the library."""
    empty_scene()
    objs = import_part(job["part"])
    if not objs:
        raise ValueError("nothing to import in %s" % os.path.basename(job["part"]))
    return catalog(objs, job)


# Name fragments that say which role an object plays, most specific first — "rear wheel"
# before "wheel", "swingarm" before "arm". A hand-kept mirror of `HINTS` in
# `bikeparts.rs`: that copy decides a *whole file's* guessed role from a weighted vote
# across every object's name; this one decides which group a full bike's *own* object
# belongs to, one object at a time, so it only needs the name itself, not a vote.
SPLIT_HINTS = [
    ("rearwheel", "wheel_r"),
    ("rwheel", "wheel_r"),
    ("wheelr", "wheel_r"),
    ("frontwheel", "wheel_f"),
    ("fwheel", "wheel_f"),
    ("wheelf", "wheel_f"),
    ("swingarm", "rsusp"),
    ("rsusp", "rsusp"),
    ("rearsusp", "rsusp"),
    ("shock", "rsusp"),
    ("linkage", "rsusp"),
    ("fsusp", "fsusp"),
    ("frontsusp", "fsusp"),
    ("fork", "fsusp"),
    ("handguard", "handguards"),
    ("brushguard", "handguards"),
    ("numberplate", "plate"),
    ("frontplate", "plate"),
    ("triple", "steer"),
    ("handlebar", "steer"),
    ("steer", "steer"),
    ("clutchlever", "levers"),
    ("brakelever", "levers"),
    ("gearlever", "levers"),
    ("shifter", "levers"),
    ("lever", "levers"),
    ("footpeg", "pedals"),
    ("pedal", "pedals"),
    ("peg", "pedals"),
    ("chassis", "chassis"),
    ("frame", "chassis"),
]


def squeeze(name):
    return "".join(c.lower() for c in name if c.isalnum())


def split_hint(name):
    s = squeeze(name)
    for key, role in SPLIT_HINTS:
        if key in s:
            return role
    return None


def group_of(obj, cache):
    """Which group `obj` falls into: its own name's hint, or — walking up — the nearest
    ancestor's. A wheel's hub bolts and an empty's own name rarely say "wheel", but the
    object they're parented to almost always does; asking the chain is what keeps those
    with the part they belong to instead of scattering them into "unassigned"."""
    if obj.name in cache:
        return cache[obj.name]
    cache[obj.name] = None  # breaks a cycle, which a well-formed scene never has anyway
    hint = split_hint(obj.name)
    if hint is None and obj.parent is not None:
        hint = group_of(obj.parent, cache)
    cache[obj.name] = hint
    return hint


def op_split(job):
    """A full bike (or a big sub-assembly) brought in as one file, cut into its parts by
    name — each object's own name if it hints at a role, else its nearest named ancestor's.
    Anything left with no hint anywhere up its chain is its own "unassigned" group, never
    silently dropped or folded into whichever group happened to import first.

    Each group gets its own standalone FBX, in `job["partsDir"]` (a folder that outlives
    this job, unlike `job["workDir"]`) — not just a GLB and a thumbnail. A later build
    imports a part's `source` fresh (`op_assemble` doesn't work from the catalogued GLB),
    and that has to be the group's own file, not the whole bike it was cut from, or building
    with a split part in a slot would import the entire bike into every slot that used one.
    The caller (`bike_part_split` in `main.rs`) turns each group into its own library part,
    with this FBX as its `source`, and removes the one part this replaces."""
    empty_scene()
    objs = import_part(job["part"])
    if not objs:
        raise ValueError("nothing to import in %s" % os.path.basename(job["part"]))
    work = job["workDir"]
    parts_dir = job["partsDir"]
    os.makedirs(parts_dir, exist_ok=True)
    cache = {}
    buckets = {}
    for o in objs:
        buckets.setdefault(group_of(o, cache) or "unassigned", []).append(o)
    if len(buckets) < 2:
        raise ValueError("this file only has one part in it — nothing to split")

    groups = []
    # Sorted so the result (and so the tag on each exported file) is the same every run,
    # not whatever order Blender happened to import the objects in.
    for i, (key, group_objs) in enumerate(sorted(buckets.items())):
        tag = key if key != "unassigned" else "part-%d" % (i + 1)
        source = os.path.join(parts_dir, "%s.fbx" % tag)
        export_fbx(source, group_objs)
        sub = catalog(
            group_objs,
            {"thumb": os.path.join(work, "%s-thumb.png" % tag), "thumbSize": job.get("thumbSize") or 256,
             "glb": os.path.join(work, "%s.glb" % tag)},
        )
        sub["tag"] = tag
        sub["role"] = None if key == "unassigned" else key
        sub["source"] = source
        groups.append(sub)
    return {"groups": groups}


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


def op_assemble(job):
    """Phase E: the slotted parts, each moved onto its anchor and carried into the frame of
    the game part it's built into, under roots named for those parts. Exported as the model's
    FBX, then again as a low, plain white shadow.

    The roots sit at the origin, unturned: the converter keeps a root's rotation and drops
    its position, and the template's .geom places each part, so all the placing is in the
    meshes. Empties are dropped; the lever and peg objects keep their names, which the
    game's gfx.cfg animates them by.
    """
    empty_scene()
    roots = {}
    for group in job["groups"]:
        root = bpy.data.objects.new(group, None)
        bpy.context.scene.collection.objects.link(root)
        roots[group] = root
    for p in job["parts"]:
        objs = import_part(p["source"])
        into = Matrix(job["groups"][p["group"]]) @ Matrix.Translation(Vector(p["offset"]))
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

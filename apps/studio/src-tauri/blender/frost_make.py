# SPDX-License-Identifier: GPL-3.0-or-later
#
# Frost Studio's bike builder: parts made from nothing, the Blender half.
#
# Copyright (C) 2026 Creste LLC
#
# This program is free software: you can redistribute it and/or modify it under the terms of
# the GNU General Public License as published by the Free Software Foundation, either version
# 3 of the License, or (at your option) any later version. It is distributed in the hope that
# it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See <https://www.gnu.org/licenses/>.
#
# Imported by frost_bike.py, which Studio runs in the rider's own Blender. Two things live here:
#
# - a placeholder bike: eight parts built from primitives, each with the attach empties the
#   builder snaps by, so the whole assembler can be tried before anyone brings real parts;
# - the Part Maker's templates: parametric parts (handguards, a number plate, grips) whose
#   sliders are plain numbers, so a person or a model can drive them.
#
# The frame is Blender's: metres, Z up, the bike facing -Y (so the rider's right is -X), and
# the ground at z = 0. Every template builds around its own mount empty at the origin; the
# builder puts that empty on its parent's anchor.

import math

import bpy
from mathutils import Vector

# Where a stock-sized 450 puts its anchors, in the frame above. The placeholder parts are
# built around these, and the builder's default template (bikeassemble.rs) holds the same
# numbers: change one, change both.
ANCHORS = {
    "steer_axis": (0.0, -0.47, 0.93),
    "fork_clamp": (0.0, -0.47, 0.93),
    "handlebar": (0.0, -0.40, 1.10),
    "swingarm_pivot": (0.0, 0.12, 0.52),
    "footpegs": (0.0, 0.08, 0.40),
    "front_axle": (0.0, -0.74, 0.365),
    "rear_axle": (0.0, 0.74, 0.35),
    "plate_mount": (0.0, -0.58, 0.98),
}

# Which anchor each role hangs from, and which it provides for the parts under it.
ROLE_MOUNT = {
    "chassis": None,
    "steer": "steer_axis",
    "fsusp": "fork_clamp",
    "rsusp": "swingarm_pivot",
    "wheel_f": "front_axle",
    "wheel_r": "rear_axle",
    "levers": "handlebar",
    "pedals": "footpegs",
    "handguards": "handlebar",
    "plate": "plate_mount",
}
ROLE_PROVIDES = {
    "chassis": ["steer_axis", "swingarm_pivot", "footpegs"],
    "steer": ["fork_clamp", "handlebar", "plate_mount"],
    "fsusp": ["front_axle"],
    "rsusp": ["rear_axle"],
}


def v(t):
    return Vector(t)


def material(name, rgba, metallic=0.0, roughness=0.5):
    mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    mat.diffuse_color = rgba
    mat.metallic = metallic
    mat.roughness = roughness
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    if bsdf is not None:
        bsdf.inputs["Base Color"].default_value = rgba
        bsdf.inputs["Metallic"].default_value = metallic
        bsdf.inputs["Roughness"].default_value = roughness
    return mat


def hex_rgba(text, fallback=(0.9, 0.35, 0.05, 1.0)):
    """"#ff6600" or "ff6600" → linear-ish RGBA. Anything else is the fallback colour."""
    t = (text or "").strip().lstrip("#")
    if len(t) != 6:
        return fallback
    try:
        r, g, b = (int(t[i:i + 2], 16) / 255.0 for i in (0, 2, 4))
    except ValueError:
        return fallback
    # sRGB to linear, so the colour asked for is the colour seen.
    lin = lambda c: c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return (lin(r), lin(g), lin(b), 1.0)


def link(obj, parent=None):
    if obj.name not in bpy.context.scene.collection.objects:
        for c in list(obj.users_collection):
            c.objects.unlink(obj)
        bpy.context.scene.collection.objects.link(obj)
    if parent is not None:
        obj.parent = parent
        # Without this, Blender leaves obj's own location/rotation as they were and the
        # parent's transform is applied on top of them, doubling it whenever the parent isn't
        # sitting at the origin with no rotation — wheel()'s hub and rim, parented onto a tyre
        # that is itself turned 90°, would otherwise end up turned 180° and pointing the wrong
        # way. This keeps obj exactly where it was built, in the parent's frame from here on.
        obj.matrix_parent_inverse = parent.matrix_world.inverted()
    return obj


def empty(name, at, parent=None):
    e = bpy.data.objects.new(name, None)
    e.empty_display_type = "PLAIN_AXES"
    e.empty_display_size = 0.05
    e.location = v(at)
    return link(e, parent)


def finish(obj, name, mat, parent=None):
    obj.name = name
    obj.data.name = name
    obj.data.materials.clear()
    obj.data.materials.append(mat)
    return link(obj, parent)


def tube(name, a, b, radius, mat, parent=None, verts=12):
    """A cylinder from point a to point b."""
    a, b = v(a), v(b)
    d = b - a
    bpy.ops.mesh.primitive_cylinder_add(vertices=verts, radius=radius, depth=d.length, location=(a + b) / 2)
    obj = bpy.context.object
    obj.rotation_euler = d.to_track_quat("Z", "Y").to_euler()
    return finish(obj, name, mat, parent)


def box(name, at, size, mat, parent=None, rot=(0, 0, 0)):
    bpy.ops.mesh.primitive_cube_add(size=1, location=v(at), rotation=rot)
    obj = bpy.context.object
    obj.scale = v(size)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return finish(obj, name, mat, parent)


def wheel(name, radius, width, mat_tyre, mat_rim, parent=None):
    bpy.ops.mesh.primitive_torus_add(
        major_radius=radius - width * 0.5, minor_radius=width * 0.5,
        major_segments=48, minor_segments=12, rotation=(0, math.pi / 2, 0),
    )
    tyre = finish(bpy.context.object, name, mat_tyre, parent)
    bpy.ops.mesh.primitive_cylinder_add(vertices=24, radius=0.06, depth=0.12, rotation=(0, math.pi / 2, 0))
    finish(bpy.context.object, name + "_hub", mat_rim, tyre)
    bpy.ops.mesh.primitive_torus_add(
        major_radius=radius - width, minor_radius=0.012,
        major_segments=48, minor_segments=6, rotation=(0, math.pi / 2, 0),
    )
    finish(bpy.context.object, name + "_rim", mat_rim, tyre)
    return tyre


# ---------------------------------------------------------------------------
# the placeholder bike


def placeholder(role):
    """Build one placeholder part in an empty scene, around the anchors above.

    A part carries its mount empty (named for the anchor it hangs from) and the anchors it
    provides, all where a real bike has them. So the parts drop into place with no nudging,
    which is what makes them a fair test of the snapping.
    """
    frame = material("frost_frame", (0.05, 0.05, 0.06, 1), metallic=0.6, roughness=0.35)
    plastic = material("frost_plastic", (0.9, 0.3, 0.02, 1), roughness=0.4)
    black = material("frost_black", (0.02, 0.02, 0.02, 1), roughness=0.8)
    alu = material("frost_alu", (0.7, 0.7, 0.72, 1), metallic=1.0, roughness=0.3)
    A = ANCHORS
    x = lambda p, dx: (p[0] + dx, p[1], p[2])

    root = empty(role, (0, 0, 0))
    mount = ROLE_MOUNT.get(role)
    if mount:
        empty(mount, A[mount], root)
    for name in ROLE_PROVIDES.get(role, []):
        empty(name, A[name], root)

    if role == "chassis":
        head, pivot = A["steer_axis"], A["swingarm_pivot"]
        tube("frame_top", head, (0, 0.25, 0.78), 0.022, frame, root)
        tube("frame_down", (0, -0.45, 0.85), (0, -0.2, 0.28), 0.024, frame, root)
        tube("frame_cradle", (0, -0.2, 0.28), (0, 0.1, 0.30), 0.022, frame, root)
        tube("frame_rear", (0, 0.1, 0.30), (0, 0.25, 0.78), 0.022, frame, root)
        tube("subframe", (0, 0.25, 0.78), (0, 0.62, 0.88), 0.014, frame, root)
        box("engine", (0, -0.05, 0.42), (0.26, 0.34, 0.3), alu, root)
        box("tank", (0, -0.12, 0.92), (0.3, 0.38, 0.16), plastic, root)
        box("seat", (0, 0.32, 0.92), (0.22, 0.62, 0.07), black, root)
        box("side_panels", (0, 0.42, 0.82), (0.28, 0.3, 0.12), plastic, root)
        tube("pivot_bolt", x(pivot, -0.12), x(pivot, 0.12), 0.012, alu, root)
    elif role == "steer":
        clamp, bar = A["steer_axis"], A["handlebar"]
        box("triple_top", (clamp[0], clamp[1] + 0.02, clamp[2] + 0.06), (0.24, 0.1, 0.03), alu, root)
        box("triple_bottom", (clamp[0], clamp[1] - 0.04, clamp[2] - 0.12), (0.24, 0.1, 0.035), alu, root)
        tube("bar_mount", (clamp[0], clamp[1] + 0.02, clamp[2] + 0.07), bar, 0.012, alu, root)
        tube("handlebar_tube", x(bar, -0.40), x(bar, 0.40), 0.011, alu, root)
        tube("grip_r", x(bar, -0.40), x(bar, -0.28), 0.017, black, root)
        tube("grip_l", x(bar, 0.28), x(bar, 0.40), 0.017, black, root)
    elif role == "fsusp":
        clamp, axle = A["fork_clamp"], A["front_axle"]
        for side, dx in (("r", -0.095), ("l", 0.095)):
            top = (dx, clamp[1] + 0.04, clamp[2] + 0.08)
            mid = (dx, (clamp[1] + axle[1]) / 2, (clamp[2] + axle[2]) / 2)
            tube("fork_tube_" + side, top, mid, 0.024, alu, root)
            tube("fork_leg_" + side, mid, (dx, axle[1], axle[2]), 0.03, black, root)
        box("front_fender", (0, axle[1] - 0.05, axle[2] + 0.43), (0.12, 0.5, 0.02), plastic, root, rot=(0.25, 0, 0))
        tube("front_axle_bolt", x(axle, -0.12), x(axle, 0.12), 0.01, alu, root)
    elif role == "rsusp":
        pivot, axle = A["swingarm_pivot"], A["rear_axle"]
        for side, dx in (("r", -0.1), ("l", 0.1)):
            tube("swingarm_" + side, (dx, pivot[1], pivot[2]), (dx, axle[1], axle[2]), 0.025, alu, root)
        tube("shock", (0, 0.2, 0.52), (0, 0.18, 0.82), 0.03, plastic, root)
        tube("rear_axle_bolt", x(axle, -0.13), x(axle, 0.13), 0.01, alu, root)
    elif role in ("wheel_f", "wheel_r"):
        axle = A["front_axle" if role == "wheel_f" else "rear_axle"]
        w = wheel("fwheel" if role == "wheel_f" else "rwheel", axle[2], 0.09 if role == "wheel_f" else 0.11, black, alu, root)
        w.location = v(axle)
    elif role == "levers":
        bar = A["handlebar"]
        box("frontbrake_lever", (bar[0] - 0.26, bar[1] - 0.07, bar[2]), (0.16, 0.02, 0.012), alu, root, rot=(0, 0, 0.3))
        box("clutch_lever", (bar[0] + 0.26, bar[1] - 0.07, bar[2]), (0.16, 0.02, 0.012), alu, root, rot=(0, 0, -0.3))
    elif role == "pedals":
        pegs = A["footpegs"]
        box("footpeg_r", (pegs[0] - 0.19, pegs[1], pegs[2]), (0.1, 0.05, 0.02), alu, root)
        box("footpeg_l", (pegs[0] + 0.19, pegs[1], pegs[2]), (0.1, 0.05, 0.02), alu, root)
        box("rearbrake_lever", (pegs[0] - 0.16, pegs[1] - 0.14, pegs[2] + 0.02), (0.02, 0.2, 0.015), alu, root)
        box("gear_lever", (pegs[0] + 0.16, pegs[1] - 0.12, pegs[2] + 0.06), (0.02, 0.16, 0.015), alu, root)
    else:
        raise ValueError("no placeholder for role %r" % role)
    return root


PLACEHOLDER_ROLES = ["chassis", "steer", "fsusp", "rsusp", "wheel_f", "wheel_r", "levers", "pedals"]


# ---------------------------------------------------------------------------
# Part Maker templates
#
# Each template is make(params) → the part's root empty, built around its mount empty at the
# origin. SLIDERS says what each takes: (name, default, min, max) for numbers, and choices for
# the rest. Studio reads the table through the `templates` op, so the UI and the prompt the
# model gets are both made from it.

def clamp(x, lo, hi):
    return max(lo, min(hi, x))


def num(params, spec):
    name, default, lo, hi = spec
    try:
        return clamp(float(params.get(name, default)), lo, hi)
    except (TypeError, ValueError):
        return default


def curve_object(name, points, bevel=0.0, extrude=0.0, resolution=12):
    """A smooth curve through `points` (a poly of handles, auto-smoothed)."""
    cu = bpy.data.curves.new(name, "CURVE")
    cu.dimensions = "3D"
    cu.resolution_u = resolution
    cu.bevel_depth = bevel
    cu.bevel_resolution = 4
    cu.extrude = extrude
    cu.use_fill_caps = True
    sp = cu.splines.new("BEZIER")
    sp.bezier_points.add(len(points) - 1)
    for bp, p in zip(sp.bezier_points, points):
        bp.co = v(p)
        bp.handle_left_type = bp.handle_right_type = "AUTO"
    obj = bpy.data.objects.new(name, cu)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def to_mesh(obj, name, mat, parent, solidify=0.0):
    bpy.ops.object.select_all(action="DESELECT")
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.convert(target="MESH")
    obj = bpy.context.object
    if solidify > 0:
        mod = obj.modifiers.new("solidify", "SOLIDIFY")
        mod.thickness = solidify
        mod.offset = 0
        bpy.ops.object.modifier_apply(modifier=mod.name)
    obj.name = name
    obj.data.name = name
    obj.data.materials.clear()
    obj.data.materials.append(mat)
    obj.parent = parent
    return obj


HANDGUARD_SLIDERS = [
    ("reach", 0.09, 0.04, 0.16),      # how far past the bar end the guard wraps, metres
    ("sweep", 0.11, 0.05, 0.2),       # how far forward of the bar the shield stands
    ("height", 0.09, 0.04, 0.16),     # the shield's height
    ("thickness", 0.004, 0.002, 0.01),
    ("bar_radius", 0.009, 0.005, 0.014),
    ("span", 0.40, 0.3, 0.45),        # half the handlebar's width, bar end to centre
]
HANDGUARD_CHOICES = {"mount": ["wrap", "open"]}


def make_handguards(params):
    """Handguards for both bar ends: a shield in front of each lever and, with the `wrap`
    mount, an aluminium backbone that runs from inside the clamp round to the bar end.

    The mount empty (`handlebar`) sits at the bar's centre, where the steer provides it.
    """
    reach, sweep, height, thick, bar_r, span = (num(params, s) for s in HANDGUARD_SLIDERS)
    wrap = params.get("mount", "wrap") != "open"
    shell = material("frost_handguard", hex_rgba(params.get("color", "#ff6600")), roughness=0.45)
    backbone = material("frost_handguard_bar", (0.75, 0.75, 0.78, 1), metallic=1.0, roughness=0.3)

    root = empty("handguards", (0, 0, 0))
    empty("handlebar", (0, 0, 0), root)
    for side, s in (("r", -1.0), ("l", 1.0)):
        inner = span - 0.16          # where the backbone clamps, inside the grip
        end = span + 0.01            # the bar end
        # Top view, the bike facing -Y: forward of the bar is -Y.
        path = [
            (s * inner, -0.02, 0.0),
            (s * (inner + 0.02), -sweep * 0.8, 0.0),
            (s * (end + reach * 0.5), -sweep, 0.0),
            (s * (end + reach), -sweep * 0.45, 0.0),
            (s * end, 0.0, 0.0),
        ]
        if wrap:
            bb = curve_object("handguard_bar_" + side, path, bevel=bar_r)
            to_mesh(bb, "handguard_bar_" + side, backbone, root)
        # The shield: the front of the path, stood up and thickened, a little above the bar.
        shield_path = [(x, y - 0.004, 0.0) for (x, y, _) in path[1:4]]
        sh = curve_object("handguard_" + side, shield_path, extrude=height * 0.5)
        sh.location = (0, 0, height * 0.18)
        # Tilted back a little, the way a real guard leans over the lever.
        sh.rotation_euler = (math.radians(-12), 0, 0)
        to_mesh(sh, "handguard_" + side, shell, root, solidify=thick)
    return root


PLATE_SLIDERS = [
    ("width", 0.24, 0.16, 0.32),
    ("height", 0.2, 0.14, 0.28),
    ("curve", 0.03, 0.0, 0.08),       # how far the plate bows forward at its middle
    ("thickness", 0.004, 0.002, 0.01),
    ("tilt", 18.0, 0.0, 35.0),        # degrees back from upright
]
PLATE_CHOICES = {}


def make_plate(params):
    """A front number plate, bowed around the forks, hanging from the bar clamp."""
    width, height, bow, thick, tilt = (num(params, s) for s in PLATE_SLIDERS)
    shell = material("frost_plate", hex_rgba(params.get("color", "#ffffff"), (1, 1, 1, 1)), roughness=0.5)
    root = empty("plate", (0, 0, 0))
    empty("plate_mount", (0, 0, 0), root)
    w = width / 2
    path = [(-w, 0.0, 0.0), (-w * 0.5, -bow * 0.8, 0.0), (0.0, -bow, 0.0), (w * 0.5, -bow * 0.8, 0.0), (w, 0.0, 0.0)]
    pl = curve_object("plate", path, extrude=height / 2)
    pl.location = (0, -0.02, -height * 0.3)
    pl.rotation_euler = (math.radians(tilt), 0, 0)
    to_mesh(pl, "plate", shell, root, solidify=thick)
    return root


GRIPS_SLIDERS = [
    ("length", 0.13, 0.1, 0.16),
    ("radius", 0.017, 0.013, 0.022),
    ("span", 0.40, 0.3, 0.45),
    ("flange", 0.024, 0.0, 0.035),
]
GRIPS_CHOICES = {}


def make_grips(params):
    length, radius, span, flange = (num(params, s) for s in GRIPS_SLIDERS)
    rubber = material("frost_grip", hex_rgba(params.get("color", "#111111"), (0.02, 0.02, 0.02, 1)), roughness=0.9)
    root = empty("grips", (0, 0, 0))
    empty("handlebar", (0, 0, 0), root)
    for side, s in (("r", -1.0), ("l", 1.0)):
        tube("grip_" + side, (s * (span - length), 0, 0), (s * span, 0, 0), radius, rubber, root, verts=16)
        if flange > 0:
            tube("grip_flange_" + side, (s * (span - length - 0.006), 0, 0), (s * (span - length), 0, 0), flange, rubber, root, verts=16)
    return root


TEMPLATES = {
    "handguards": {"role": "handguards", "make": make_handguards, "sliders": HANDGUARD_SLIDERS, "choices": HANDGUARD_CHOICES,
                   "about": "Wraparound or open handguards for both bar ends, with a coloured shield."},
    "plate": {"role": "plate", "make": make_plate, "sliders": PLATE_SLIDERS, "choices": PLATE_CHOICES,
              "about": "A front number plate that hangs from the bar clamp."},
    "grips": {"role": "levers", "make": make_grips, "sliders": GRIPS_SLIDERS, "choices": GRIPS_CHOICES,
              "about": "Handlebar grips with an optional flange."},
}


def describe_templates():
    return {
        name: {
            "role": t["role"],
            "about": t["about"],
            "sliders": [{"name": n, "default": d, "min": lo, "max": hi} for (n, d, lo, hi) in t["sliders"]],
            "choices": t["choices"],
        }
        for name, t in TEMPLATES.items()
    }

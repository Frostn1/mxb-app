# SPDX-License-Identifier: GPL-3.0-or-later
#
# Frost Studio's bike builder: reading part files laid out the way mxb-blender-kit (and so every
# Frost Blend) writes them. Copyright (C) 2026 Creste LLC. GPL-3.0-or-later, like frost_bike.py
# beside it, which imports this. No `bpy` in here: it decides from plain names and numbers, so
# test_frost_kit.py runs it with any Python.
#
# A kit game file holds the same part four times over, one collection each:
#
#     <page>_LOD0     LOD0_wp_steer, LOD0_wp_fsusp, LOD0_wp_cap_air_L ...
#     <page>_LOD1     the same objects, names + "b"
#     <page>_LOD2     the same objects, names + "c"
#     <page>_SHADOW   the same objects, names + "_shadow"
#
# and each LOD0 object carries an `mxb_node` custom property: the game node (steer, fsusp ...)
# the kit parents it to for the EDF. Studio wants LOD0 only, and one part per node.

import re

LEVELS = ("LOD0", "LOD1", "LOD2", "SHADOW")
# The kit's object name suffix per level (mxb_blender_kit.finalize.bl_game.SUFFIX).
SUFFIX = {"LOD1": "b", "LOD2": "c", "SHADOW": "_shadow"}
_COLL = re.compile(r"(?:^|[^a-z0-9])(lod[0-9]|shadow)$", re.I)
# Blender's own ".001" on a name clash, which an FBX import or an append can add.
_DUP = re.compile(r"\.\d{3}$")

# Name fragments that say which role an object plays, most specific first: "rear wheel"
# before "wheel", "swingarm" before "arm". A hand-kept mirror of `HINTS` in `bikeparts.rs`.
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
    s = squeeze(name or "")
    for key, role in SPLIT_HINTS:
        if key in s:
            return role
    return None


def collection_level(name):
    """"wp7448_LOD1" -> "LOD1", "seat_SHADOW" -> "SHADOW", "LOD0" -> "LOD0"; else None."""
    m = _COLL.search(name or "")
    if not m:
        return None
    level = m.group(1).upper()
    return level if level in LEVELS else None


def _name_levels(names):
    """Level per object from the kit's suffixes alone, for a file whose collections were lost
    (an FBX export of a kit file): "x" is LOD0 when "xb", "xc" or "x_shadow" sit beside it.
    Only trusted when at least two of the lower levels show up, so a lone "pin"/"pinb" pair in
    an ordinary file isn't taken for a LOD set."""
    bare = {_DUP.sub("", n): n for n in names}
    levels = {}
    for stem, name in bare.items():
        for level, suffix in SUFFIX.items():
            if stem.endswith(suffix) and stem[: -len(suffix)] in bare:
                levels[name] = level
                break
    found = set(levels.values())
    if len(found) < 2:
        return {}
    return levels


def pick_lod0(objects):
    """Which objects of a part file to keep: LOD0 only, when the file has LODs.

    `objects`: dicts with "name", "collections" (names of the collections it's in, may be
    empty) and "tris". Returns (keep, report): `keep` is the set of names to import, `report`
    is None for a file without LODs (keep is then every object), else
    {"tris": LOD0 triangles, "leftOut": ["LOD1", "LOD2", "SHADOW"] (those found), "by": ...}.

    Collections decide first (`<page>_LOD0` ...); an object in no LOD collection at all (an
    attach empty at the scene root) is kept. Without LOD collections the kit's name suffixes
    decide."""
    names = [o["name"] for o in objects]
    levels = {}
    by = None
    for o in objects:
        found = {collection_level(c) for c in o.get("collections") or ()} - {None}
        if found:
            # In LOD0 and some other level both (shared, which the kit never does): keep it.
            levels[o["name"]] = "LOD0" if "LOD0" in found else sorted(found)[0]
    if "LOD0" in levels.values() and set(levels.values()) - {"LOD0"}:
        by = "collections"
    else:
        levels = _name_levels(names)
        if levels:
            by = "names"
    if by is None:
        return set(names), None
    keep = {n for n in names if levels.get(n, "LOD0") == "LOD0"}
    left_out = [lv for lv in LEVELS[1:] if lv in levels.values()]
    tris = sum(int(o.get("tris") or 0) for o in objects if o["name"] in keep)
    return keep, {"tris": tris, "leftOut": left_out, "by": by}


def group_objects(objects):
    """Which group (a role, or None) each object goes in when a file is cut into parts.

    `objects`: dicts with "name", "parent" (a name or None) and "node" (the kit's `mxb_node`,
    or None). An object's group is its `mxb_node`'s role (the game node the kit parents it to
    for the EDF: a fork's caps, valves and screws say `steer`), else its own name's hint, else
    its nearest ancestor's. Nothing is placed by where it sits: on a fork the inner tube runs
    up into the caps, so the closest mesh to a cap is the fsusp, not the steer the kit puts it
    on."""
    by_name = {o["name"]: o for o in objects}
    cache = {}

    def own(o):
        node = o.get("node")
        if node:
            role = split_hint(node)
            if role:
                return role
        return split_hint(o["name"])

    def group_of(name):
        if name in cache:
            return cache[name]
        cache[name] = None  # breaks a cycle, which a well-formed scene never has anyway
        o = by_name[name]
        g = own(o)
        if g is None and o.get("parent") in by_name:
            g = group_of(o["parent"])
        cache[name] = g
        return g

    return {o["name"]: group_of(o["name"]) for o in objects}


# ---------------------------------------------------------------------------
# splitting a file that is one mesh
#
# A part with no kit nodes and no role names to go by can still come apart along its own
# geometry: by material (a fork leg's chrome tube and its anodised outer) or by loose piece
# (bolts, caps and guards modelled as separate shells in one object). These decide the names
# and how many pieces are worth keeping; frost_bike.py does the cutting.

# More pieces than this and a tray of parts stops being useful: the largest keep their own
# part and the rest go together as one "<name>-rest".
MAX_PIECES = 24
_UNSAFE = re.compile(r"[^A-Za-z0-9_.-]+")


def safe_tag(name, fallback="part"):
    """A piece's name as a file name: material names carry spaces, dots and slashes."""
    tag = _UNSAFE.sub("_", _DUP.sub("", name or "")).strip("._")
    return tag[:60] or fallback


def unique_tags(names):
    """`safe_tag` each name, numbering repeats: two pieces of "Chrome" become Chrome-1, Chrome-2,
    a lone one stays plain Chrome."""
    tags = [safe_tag(n) for n in names]
    counts = {}
    for t in tags:
        counts[t.lower()] = counts.get(t.lower(), 0) + 1
    seen = {}
    out = []
    for t in tags:
        k = t.lower()
        if counts[k] > 1:
            seen[k] = seen.get(k, 0) + 1
            out.append("%s-%d" % (t, seen[k]))
        else:
            out.append(t)
    return out


def keep_largest(pieces, limit=MAX_PIECES):
    """`pieces` as (name, tris): the indexes kept on their own, largest first, and the indexes
    folded into one "rest" piece (empty when everything fits)."""
    order = sorted(range(len(pieces)), key=lambda i: (-pieces[i][1], pieces[i][0]))
    if len(order) <= limit:
        return order, []
    return order[: limit - 1], order[limit - 1:]


# Principled BSDF inputs the converter reads a game texture from, by what it becomes: the
# colour sheet (FBX DiffuseColor), the spec map (ShininessExponent, from Roughness; or the
# Specular inputs) and the normal map (through a Normal Map node).
COLOUR_INPUTS = ("Base Color",)
SPEC_INPUTS = ("Roughness", "Specular IOR Level", "Specular", "Metallic")
NORMAL_INPUTS = ("Normal",)
# Inputs that blend between colours rather than carry one: a mask driving a Mix is not the
# colour, even when it's an image.
_MASK_INPUTS = ("Factor", "Fac")


def _upstream_image(links, images, node, sockets, skip=()):
    """The nearest image node feeding `node` through any of `sockets`, walking back through
    whatever sits between (Mix, Map Range, Normal Map ...), never through a mask input, nor to
    an image in `skip`. `links`: (from_node, to_node, to_socket); `images`: the image nodes."""
    frontier = [f for f, t, s in links if t == node and s in sockets]
    seen = set()
    while frontier:
        found = [n for n in frontier if n in images and n not in skip]
        if found:
            return sorted(found)[0]
        nxt = []
        for n in frontier:
            if n in seen:
                continue
            seen.add(n)
            nxt += [f for f, t, s in links if t == n and s not in _MASK_INPUTS]
        frontier = nxt
    return None


def game_maps(links, images, bsdf):
    """Which image node is a material's colour, spec and normal map, as the game reads them,
    however the material wires them for Blender's own view. A Frost Blend's view material mixes
    the colour sheet and puts the spec map through Map Range nodes; the FBX exporter only
    follows an image wired straight into the shader, so without this the colour and spec maps
    never reach the converter and the part comes out untextured.

    Returns {"colour": node or None, "spec": ..., "normal": ...}."""
    colour = _upstream_image(links, images, bsdf, COLOUR_INPUTS)
    # The spec map is an image of its own: a Mix's mask taken from it doesn't make it colour.
    spec = None
    for s in SPEC_INPUTS:
        spec = _upstream_image(links, images, bsdf, (s,), skip=(colour,))
        if spec:
            break
    normal = _upstream_image(links, images, bsdf, NORMAL_INPUTS, skip=(colour, spec))
    return {"colour": colour, "spec": spec, "normal": normal}

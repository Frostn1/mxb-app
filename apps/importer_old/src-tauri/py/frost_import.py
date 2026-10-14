"""Frost Importer's Blender side: build a .blend from a plan the app writes.

    blender -b --factory-startup --python-exit-code 1 --python frost_import.py -- \
        --spec plan.json --report report.json

The plan names every EDF object to load and the part it belongs to (see the app's plan.rs). For a
locked .pkz the app passes `--stdin` instead of `--spec`: the plan and the files it names arrive on
stdin and stay in memory, and every texture is packed into the .blend (`pack`), none written beside.

The EDF reader and the mesh/material build are the pipeline's (mxb-blender-kit's edf.reader and
edf.blender_import, which the KTM refs, the material-test legend and the Airoh render were made
with), made lenient where shipped mods are: a texture section or EXT object that fails to parse is
reported and skipped, an empty (0x0) map is skipped.

Axes: game is left-handed +X right, +Y up, +Z forward. Blender = (-x, -z, y): the inverse of the
FBX export plus fbx2edf, so a round trip lines up and the bike faces Blender -Y.

Parts are placed from the bike's .geom the way the game and MXB App's viewer do
(mxb_core::edf::assemble_bike): chassis as it is, the swingarm by its pivot, the steer and the
fork turned by the rake about the steering head.
"""
import json
import os
import re
import struct
import sys
import time
import zlib

import numpy as np

# Files the app piped in (`--stdin`), by the path the plan names them with.
FILES = {}


def read_bytes(path):
    if path in FILES:
        return FILES[path]
    with open(path, "rb") as fh:
        return fh.read()


def read_piped(stream):
    """The app's stdin: u32 + plan JSON, u32 count, then per file u32 + path, u64 + bytes."""

    def take(n):
        b = stream.read(n)
        if len(b) != n:
            raise EOFError("the app's data stopped short")
        return b

    spec = json.loads(take(struct.unpack("<I", take(4))[0]).decode("utf-8"))
    for _ in range(struct.unpack("<I", take(4))[0]):
        path = take(struct.unpack("<I", take(4))[0]).decode("utf-8")
        FILES[path] = take(struct.unpack("<Q", take(8))[0])
    return spec


DUMMY = 0x1000
DEFORM = 0x10000


def progress(done, total, stage):
    print(f"FI_PROGRESS {done} {total} {stage}", flush=True)


# --------------------------------------------------------------------------- EDF reader (no bpy)


class Reader:
    def __init__(self, b, o=0):
        self.b, self.o = b, o

    def need(self, n):
        if self.o + n > len(self.b):
            raise ValueError(f"read of {n} bytes past the end at {self.o:#x}")

    def u32(self):
        self.need(4)
        v = struct.unpack_from("<I", self.b, self.o)[0]
        self.o += 4
        return v

    def f32(self):
        self.need(4)
        v = struct.unpack_from("<f", self.b, self.o)[0]
        self.o += 4
        return v

    def arr(self, dt, n):
        self.need(4 * n)
        v = np.frombuffer(self.b, dt, n, self.o)
        self.o += 4 * n
        return v

    def raw(self, n):
        self.need(n)
        v = self.b[self.o:self.o + n]
        self.o += n
        return v

    def name(self):
        raw = self.raw(100)
        return raw.split(b"\0", 1)[0].decode("latin-1").rstrip("\xcd")

    def flag(self):
        at = self.o
        v = self.u32()
        if v not in (0, 1):
            raise ValueError(f"expected 0/1 at {at:#x}, got {v}")
        return v == 1


def read_node(r):
    n = {"name": r.name(), "flags": r.u32()}
    n["m"] = r.arr("<f4", 16).astype(np.float64).reshape(4, 4)
    r.raw(64)
    n["parent"] = r.u32()
    n["children"] = [int(x) for x in r.arr("<u4", r.u32())]
    ng = r.u32()
    r.u32()
    n["groups"] = [dict(zip(("mat", "t0", "tn", "v0", "vn", "tail"), (int(x) for x in r.arr("<u4", 6))))
                   for _ in range(ng)]
    n["first_vertex"] = r.u32()
    if n["flags"] & DEFORM:
        nv = sum(g["vn"] for g in n["groups"])
        kind, k = r.u32(), r.u32()
        if kind != 1:
            raise ValueError(f"node {n['name']!r}: deform type {kind} not decoded")
        n["deform_keys"] = k
        r.raw(4 * (k * nv * 10 + k))
    n["bounds"] = r.arr("<f4", 6).astype(np.float64)
    return n


def read_object(r, name):
    o = {"name": name or "(main)", "ext": name, "offset": r.o}
    o["bounds"] = r.arr("<f4", 6).astype(np.float64)
    mc = r.u32()
    o["materials"] = []
    for _ in range(mc):
        w = r.arr("<u4", 14)
        f = np.frombuffer(w.tobytes(), "<f4")
        o["materials"].append({"flags": int(w[0]), "ambient": f[1:4].tolist(), "diffuse": f[4:7].tolist(),
                               "tex": int(w[11]), "overlay": int(w[12]), "companion": int(w[13])})
    vc = r.u32()
    lanes = r.arr("<f4", vc * 18)
    o["pos"] = lanes[0:vc * 3].reshape(vc, 3).astype(np.float64)
    o["uv0"] = lanes[vc * 3:vc * 5].reshape(vc, 2)
    o["uv1"] = lanes[vc * 5:vc * 7].reshape(vc, 2)
    o["nrm"] = lanes[vc * 11:vc * 14].reshape(vc, 3).astype(np.float64)
    tc = r.u32()
    o["tri"] = r.arr("<u4", tc * 3).reshape(tc, 3).astype(np.int64)
    o["nodes"] = [read_node(r) for _ in range(r.u32())]
    o["end"] = r.o
    return o


def read_image(r, colour):
    im = {"name": r.name()}
    im["flags"] = r.u32() if colour else 0
    im["w"], im["h"] = r.u32(), r.u32()
    r.raw(16)
    first = r.u32()
    if first == 0:
        size = r.u32()
        r.u32(), r.u32()
        im["fmt"] = "RGBA"
    else:
        size = first
        fmt, _ = r.u32(), r.u32()
        im["fmt"] = {2: "L8", 5: "CUBE_RGB"}.get(fmt, f"fmt{fmt}")
    im["data"] = r.raw(size - 8)
    return im


def read_texture(r):
    t = {"colour": read_image(r, True), "maps": {}}
    t["shader"] = sh = r.u32()
    if sh not in (0, 1, 2):
        raise ValueError(f"shader word {sh}")
    if sh >= 1:
        if r.flag():
            t["reflection"] = [r.f32(), r.f32(), r.f32()]
            if r.flag():
                t["maps"]["reflection"] = read_image(r, False)
            if r.u32() != 0:
                t["maps"]["envmap"] = read_image(r, False)
        if r.flag():
            t["specular"] = r.f32()
            if r.flag():
                t["maps"]["specular"] = read_image(r, False)
        if r.flag():
            t["bump"] = r.f32()
            t["maps"]["bump"] = read_image(r, False)
        if sh == 2:
            r.u32(), r.u32()
    while r.flag():  # paint template: skipped
        r.raw(156)
        r.u32(), r.u32(), r.u32()
        r.raw(16)
        n = r.u32()
        r.raw(16 * n)
        for i in range(n):
            if i == 0:
                r.u32()
            size = r.u32()
            r.u32(), r.u32()
            r.raw(size - 8)
    return t


def read_edf(path):
    """Parse a whole plain .edf. Problems that don't stop the read go in `issues`."""
    b = read_bytes(path)
    if b[:4] != b"EDF\0":
        raise ValueError("not a plain EDF")
    ext, tex_at = [], None
    if len(b) > 8 and b[-8:-4] == b"EXT\0":
        size = struct.unpack_from("<I", b, len(b) - 4)[0]
        e = Reader(b, len(b) - size)
        tex_at = e.u32()
        for _ in range(e.u32()):
            ext.append((e.name(), e.u32()))
    r = Reader(b, 4)
    objs = [read_object(r, "")]
    issues = []
    if tex_at is not None and r.o != tex_at:
        issues.append(f"main object ends at {r.o:#x}, textures listed at {tex_at:#x}")
        r.o = tex_at
    tex = []
    try:
        for _ in range(r.u32()):
            tex.append(read_texture(r))
    except Exception as ex:
        issues.append(f"textures stopped after {len(tex)}: {ex}")
    for name, at in ext:
        try:
            objs.append(read_object(Reader(b, at), name))
        except Exception as ex:
            issues.append(f"object {name!r} skipped: {ex}")
    return {"path": path, "objects": objs, "textures": tex, "issues": issues}


def decode_image(im):
    """-> uint8 (h, w, c), row 0 = first stored row; None if not decodable or empty."""
    w, h = im["w"], im["h"]
    if not (w and h):
        return None
    try:
        px = zlib.decompress(im["data"], -15)
    except zlib.error:
        return None
    if im["fmt"] == "RGBA" and len(px) >= w * h * 4:
        return np.frombuffer(px, np.uint8, w * h * 4).reshape(h, w, 4)
    if im["fmt"] == "L8" and len(px) >= w * h:
        return np.frombuffer(px, np.uint8, w * h).reshape(h, w, 1)
    return None


def node_vertex_range(n):
    if not n["groups"]:
        return 0, 0
    return min(g["v0"] for g in n["groups"]), max(g["v0"] + g["vn"] for g in n["groups"])


# --------------------------------------------------------------------------- .shd / .geom


def parse_shd(text):
    """{'specular': {'shininess': '30'}, 'reflection': {...}, 'bump': {...}} (values as strings)."""
    out = {}
    for block, body in re.findall(r"(\w+)\s*\{([^}]*)\}", text):
        out[block.lower()] = {k.lower(): v for k, v in re.findall(r"(\w+)\s*=\s*([^\s}]+)", body)}
    return out


def parse_geom(path):
    vec, sc = {}, {}
    for line in read_bytes(path).decode("latin-1").splitlines():
        m = re.match(r"\s*([A-Za-z_][\w-]*)\s*=\s*(.+?)\s*$", line)
        if not m:
            continue
        parts = [p.strip() for p in m.group(2).split(",")]
        try:
            nums = [float(p) for p in parts]
        except ValueError:
            continue
        if len(nums) == 3:
            vec[m.group(1)] = np.array(nums)
        elif len(nums) == 1:
            sc[m.group(1)] = nums[0]
    return vec, sc


# game frame -> Blender frame
C = np.array([[-1, 0, 0, 0], [0, 0, -1, 0], [0, 1, 0, 0], [0, 0, 0, 1]], dtype=np.float64)
C3 = C[:3, :3]


def to_blender_matrix(m):
    return C @ m @ C.T


def rot_x(deg):
    s, c = np.sin(np.radians(deg)), np.cos(np.radians(deg))
    return np.array([[1, 0, 0], [0, c, -s], [0, s, c]])


def affine(rot, offset):
    m = np.eye(4)
    m[:3, :3] = rot
    m[:3, 3] = offset
    return m


def part_frames(geom):
    """Game-frame placement (4x4) of chassis / steer / fsusp / rsusp, from a .geom. None if it
    lacks the mounts a bike is placed by."""
    vec, sc = parse_geom(geom)
    need = ("chassis_steer", "chassis_rsusp_min", "steer_joint", "rsusp_joint", "front_upper")
    if not all(k in vec for k in need):
        return None
    head, pivot = vec["chassis_steer"], vec["chassis_rsusp_min"]
    sj, rj, fu = vec["steer_joint"], vec["rsusp_joint"], vec["front_upper"]
    R = rot_x(-sc.get("rakeangle_min", 0.0))
    fork_origin = R @ (fu - sj) + head
    return {
        "chassis": np.eye(4),
        "rsusp": affine(np.eye(3), pivot - rj),
        "steer": affine(R, head - R @ sj),
        "fsusp": affine(R, fork_origin),
    }


def axles(geom):
    """The front and rear axle in the game frame, from a .geom ({} if it doesn't say)."""
    vec, sc = parse_geom(geom)
    need = ("chassis_steer", "chassis_rsusp_min", "steer_joint", "rsusp_joint", "front_upper", "fwheel", "rwheel_min")
    if not all(k in vec for k in need):
        return {}
    R = rot_x(-sc.get("rakeangle_min", 0.0))
    rear = (vec["rwheel_min"] + vec.get("rwheel_max", vec["rwheel_min"])) / 2
    return {
        "axle_front": R @ (vec["fwheel"] + vec["front_upper"] - vec["steer_joint"]) + vec["chassis_steer"],
        "axle_rear": rear - vec["rsusp_joint"] + vec["chassis_rsusp_min"],
    }


# --------------------------------------------------------------------------- Blender side


class Build:
    def __init__(self, bpy, spec):
        self.bpy = bpy
        self.spec = spec
        self.tex_dir = spec["tex_dir"]
        self.pack = bool(spec.get("pack"))
        self.models = {}       # edf path -> parsed
        self.images = {}       # (edf path, texture index) -> {role: image}
        self.mat_cache = {}
        self.loose = {}        # lower stem -> path of a loose texture file
        self.loose_images = {}
        self.shd = {}          # lower stem -> parsed .shd
        self.warnings = []
        self.part_nodes = {}
        for p in spec.get("images", []):
            self.loose.setdefault(os.path.splitext(os.path.basename(p))[0].lower(), p)
        for p in spec.get("shd", []):
            try:
                self.shd[os.path.splitext(os.path.basename(p))[0].lower()] = parse_shd(read_bytes(p).decode("latin-1"))
            except OSError:
                pass

    def warn(self, text):
        self.warnings.append(text)
        print("FI_WARN", text, flush=True)

    def model(self, path):
        if path not in self.models:
            m = read_edf(path)
            for w in m["issues"]:
                self.warn(f"{os.path.basename(path)}: {w}")
            self.models[path] = m
        return self.models[path]

    # ---- images

    def _save_png(self, name, arr, non_color):
        bpy = self.bpy
        h, w, c = arr.shape
        if c == 1:
            rgba = np.concatenate([np.repeat(arr, 3, axis=2), np.full((h, w, 1), 255, np.uint8)], axis=2)
        else:
            rgba = arr
        if self.pack:
            img = bpy.data.images.new(name, w, h, alpha=True)
            img.pixels.foreach_set((rgba[::-1].astype(np.float32) / 255.0).ravel())
            img.pack()
            if non_color:
                img.colorspace_settings.name = "Non-Color"
            return img
        os.makedirs(self.tex_dir, exist_ok=True)
        path = os.path.join(self.tex_dir, f"{safe(name)}.png")
        img = bpy.data.images.new(name, w, h, alpha=True)
        img.pixels.foreach_set((rgba[::-1].astype(np.float32) / 255.0).ravel())
        img.filepath_raw = path
        img.file_format = "PNG"
        img.save()
        bpy.data.images.remove(img)
        img = bpy.data.images.load(path, check_existing=True)
        img.name = name
        if non_color:
            img.colorspace_settings.name = "Non-Color"
        return img

    def loose_image(self, stem, non_color):
        """A texture shipped as its own file (tga/dds/png), copied beside the .blend."""
        src = self.loose.get(stem.lower())
        if not src:
            return None
        if src in self.loose_images:
            return self.loose_images[src]
        if self.pack:
            img = self.packed_image(src)
            if img is not None and non_color:
                img.colorspace_settings.name = "Non-Color"
            self.loose_images[src] = img
            return img
        import shutil
        os.makedirs(self.tex_dir, exist_ok=True)
        dst = os.path.join(self.tex_dir, os.path.basename(src))
        if not os.path.exists(dst):
            shutil.copyfile(src, dst)
        try:
            img = self.bpy.data.images.load(dst, check_existing=True)
        except RuntimeError as ex:
            self.warn(f"{os.path.basename(src)}: {ex}")
            img = None
        if img is not None and non_color:
            img.colorspace_settings.name = "Non-Color"
        self.loose_images[src] = img
        return img

    def packed_image(self, src):
        """A texture file's bytes, packed into the .blend as they are: nothing is written."""
        base = os.path.basename(src)
        try:
            data = read_bytes(src)
            img = self.bpy.data.images.new(base, 1, 1)
            img.pack(data=data, data_len=len(data))
            img.source = "FILE"
            img.filepath_raw = "//" + base
            if not img.has_data:
                raise RuntimeError("couldn't read the image")
            return img
        except (OSError, RuntimeError) as ex:
            self.warn(f"{base}: {ex}")
            return None

    def texture_images(self, path, ti):
        key = (path, ti)
        if key in self.images:
            return self.images[key]
        model = self.models[path]
        t = model["textures"][ti - 1]
        imgs = {}
        cname = strip_ext(t["colour"]["name"])
        shd = self.shd.get(cname.lower(), {})
        for role, im in [("colour", t["colour"])] + list(t["maps"].items()):
            if role == "envmap":
                continue
            stem = strip_ext(im["name"]) or f"{cname}_{role}"
            img = self.loose_image(stem, role != "colour") if self.spec.get("prefer_loose", True) else None
            if img is None:
                arr = decode_image(im)
                if arr is None:
                    continue
                img = self._save_png(stem, arr, role != "colour")
            imgs[role] = img
        # Loose maps a .shd names that the EDF didn't carry.
        for role, block in (("bump", "bump"), ("reflection", "reflection"), ("specular", "specular")):
            mp = shd.get(block, {}).get("map")
            if role not in imgs and mp:
                img = self.loose_image(strip_ext(mp), True)
                if img is not None:
                    imgs[role] = img
        self.images[key] = imgs
        return imgs

    # ---- materials

    def material(self, path, m):
        bpy = self.bpy
        model = self.models[path]
        ti = m["tex"]
        tex = model["textures"][ti - 1] if 0 < ti <= len(model["textures"]) else None
        tname = strip_ext(tex["colour"]["name"]) if tex else ("texture%d" % ti if ti else "untextured")
        key = (tname, m["overlay"], tuple(round(x, 4) for x in m["diffuse"]))
        if key in self.mat_cache:
            return self.mat_cache[key]
        name = tname if not any(k[0] == tname for k in self.mat_cache) else f"{tname}.{len(self.mat_cache)}"
        mat = bpy.data.materials.new(name)
        mat["edf_texture"] = tname
        mat["edf_overlay_index"] = m["overlay"]
        mat["edf_diffuse"] = list(m["diffuse"])
        mat.diffuse_color = (*m["diffuse"], 1.0)
        try:
            mat.use_nodes = True
        except Exception:
            pass
        nt = mat.node_tree
        bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        bsdf.inputs["Base Color"].default_value = (*m["diffuse"], 1.0)
        imgs = self.texture_images(path, ti) if tex else {}

        def tex_node(img, x, y):
            n = nt.nodes.new("ShaderNodeTexImage")
            n.image = img
            n.location = (x, y)
            return n

        if "colour" in imgs:
            n = tex_node(imgs["colour"], -700, 300)
            nt.links.new(n.outputs["Color"], bsdf.inputs["Base Color"])
            flags = tex["colour"].get("flags", 0)
            mat["edf_texture_flags"] = flags
            if flags & 3:
                nt.links.new(n.outputs["Alpha"], bsdf.inputs["Alpha"])
                for attr, value in (("surface_render_method", "DITHERED"), ("blend_method", "HASHED")):
                    try:
                        setattr(mat, attr, value)
                        break
                    except Exception:
                        pass
        if tex and "specular" in tex:
            sh = tex["specular"]
            bsdf.inputs["Roughness"].default_value = float(min(1.0, (2.0 / (sh + 2.0)) ** 0.5))
            mat["edf_shininess"] = sh
        if "specular" in imgs:
            sock = bsdf.inputs.get("Specular IOR Level") or bsdf.inputs.get("Specular")
            nt.links.new(tex_node(imgs["specular"], -700, 0).outputs["Color"], sock)
        if tex and "reflection" in tex:
            rf = tex["reflection"]
            mat["edf_reflection"] = list(rf)
            if "reflection" in imgs:
                n = tex_node(imgs["reflection"], -700, -300)
                mul = nt.nodes.new("ShaderNodeMath")
                mul.operation = "MULTIPLY"
                mul.inputs[1].default_value = rf[1]
                mul.location = (-350, -300)
                nt.links.new(n.outputs["Color"], mul.inputs[0])
                nt.links.new(mul.outputs[0], bsdf.inputs["Metallic"])
        if "bump" in imgs:
            n = tex_node(imgs["bump"], -900, -600)
            nm = nt.nodes.new("ShaderNodeNormalMap")
            nm.location = (-400, -600)
            nt.links.new(n.outputs["Color"], nm.inputs["Color"])
            nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
        self.mat_cache[key] = mat
        return mat

    # ---- objects

    def edf_object(self, path, obj, coll, root, suffix=""):
        """One Blender object per EDF node, parented as in the file, under `root`."""
        from mathutils import Matrix

        bpy = self.bpy
        mats = [self.material(path, m) for m in obj["materials"]]
        has_uv1 = bool(np.any(obj["uv1"]))
        created = []
        for i, n in enumerate(obj["nodes"]):
            vs, ve = node_vertex_range(n)
            nm = (n["name"] or f"node{i}") + suffix
            ob = None
            if n["groups"] and ve > vs:
                try:
                    ob = bpy.data.objects.new(nm, self._mesh(nm, obj, n, vs, ve, mats, has_uv1))
                except Exception as ex:
                    self.warn(f"{nm}: {ex}")
            if ob is None:
                ob = bpy.data.objects.new(nm, None)
                ob.empty_display_type = "PLAIN_AXES"
                ob.empty_display_size = 0.05
            ob["edf_name"] = n["name"]
            ob["edf_flags"] = n["flags"]
            ob["edf_object"] = obj["ext"] or ""
            if "deform_keys" in n:
                ob["edf_deform_keys"] = n["deform_keys"]
            coll.objects.link(ob)
            ob.parent = created[n["parent"] - 1] if 0 < n["parent"] <= len(created) else root
            ob.matrix_parent_inverse = Matrix.Identity(4)
            ob.matrix_basis = Matrix(to_blender_matrix(n["m"]).tolist())
            created.append(ob)
        return created

    def _mesh(self, nm, obj, n, vs, ve, mats, has_uv1):
        bpy = self.bpy
        tris, mat_idx, slot_of = [], [], {}
        for g in n["groups"]:
            t = obj["tri"][g["t0"]:g["t0"] + g["tn"]]
            tris.append(t)
            slot_of.setdefault(g["mat"], len(slot_of))
            mat_idx.append(np.full(len(t), slot_of[g["mat"]], np.int32))
        tris = np.concatenate(tris).astype(np.int64) - vs
        if len(tris) == 0 or tris.min() < 0 or tris.max() >= ve - vs:
            raise ValueError("draws outside its vertex range")
        pos = obj["pos"][vs:ve] @ C3.T
        nrm = obj["nrm"][vs:ve] @ C3.T
        me = bpy.data.meshes.new(nm)
        me.vertices.add(len(pos))
        me.vertices.foreach_set("co", pos.astype(np.float32).ravel())
        ntri = len(tris)
        me.loops.add(ntri * 3)
        me.loops.foreach_set("vertex_index", tris.astype(np.int32).ravel())
        me.polygons.add(ntri)
        me.polygons.foreach_set("loop_start", np.arange(0, ntri * 3, 3, dtype=np.int32))
        me.polygons.foreach_set("material_index", np.concatenate(mat_idx))
        me.polygons.foreach_set("use_smooth", np.ones(ntri, bool))
        me.update(calc_edges=True)
        for uvname, uvs in (("UVMap", obj["uv0"]), ("UV1", obj["uv1"] if has_uv1 else None)):
            if uvs is None:
                continue
            uv = uvs[vs:ve].astype(np.float32).copy()
            uv[:, 1] = 1.0 - uv[:, 1]
            me.uv_layers.new(name=uvname).data.foreach_set("uv", uv[tris.ravel()].ravel())
        for mi in slot_of:
            me.materials.append(mats[mi] if mi < len(mats) else None)
        if np.all(np.linalg.norm(nrm, axis=1) > 0.5):
            try:
                me.normals_split_custom_set_from_vertices([tuple(v) for v in nrm])
            except Exception:
                pass
        me.validate(clean_customdata=False)
        return me

    def objects_of(self, path, name):
        """The EDF objects `name` picks: None = all, "" = the main object, else that EXT object."""
        model = self.model(path)
        if name is None:
            return model["objects"]
        want = (name or "").lower()
        found = [o for o in model["objects"] if (o["ext"] or "").lower() == want]
        if not found:
            self.warn(f"{os.path.basename(path)}: no object {name!r}")
        return found


def strip_ext(name):
    base, ext = os.path.splitext(name or "")
    return base if ext.lower() in (".tga", ".dds", ".png", ".bmp", ".jpg", ".jpeg") else (name or "")


def safe(name):
    return re.sub(r'[<>:"/\\|?*\x00-\x1f]', "_", name) or "texture"


def new_collection(bpy, name, parent):
    c = bpy.data.collections.new(name)
    parent.children.link(c)
    return c


def hide_layer(bpy, coll):
    def walk(lc):
        if lc.collection == coll:
            return lc
        for ch in lc.children:
            f = walk(ch)
            if f:
                return f
        return None

    lc = walk(bpy.context.view_layer.layer_collection)
    if lc:
        lc.hide_viewport = True
    coll.hide_render = True


def add_markers(bpy, b, part, root, coll, nodes, suffix=""):
    """gfx.cfg entries of a part: levers get their axis and travel, positions become empties."""
    from mathutils import Vector

    by_name = {(o.get("edf_name") or "").lower(): o for o in nodes}
    for mk in part.get("markers", []):
        sec = mk["section"]
        target = mk.get("name")
        if target and target.lower() in by_name:
            ob = by_name[target.lower()]
            ob["gfx_entry"] = sec
            if mk.get("axis"):
                ob["gfx_axis"] = mk["axis"]
            if mk.get("maxrot") is not None:
                ob["gfx_maxrot"] = mk["maxrot"]
        if mk.get("pos") is not None and not suffix:
            e = bpy.data.objects.new(f"gfx_{part['name']}_{sec}", None)
            e.empty_display_type = "SPHERE"
            e.empty_display_size = 0.02
            e["gfx_entry"] = sec
            coll.objects.link(e)
            e.parent = root
            g = mk["pos"]
            e.location = Vector((-g[0], -g[2], g[1]))


def build_edf_scene(bpy, b, spec):
    scene = bpy.context.scene
    top = new_collection(bpy, spec["name"], scene.collection)
    frames = None
    if spec.get("place") and spec.get("geom"):
        try:
            frames = part_frames(spec["geom"])
        except OSError as ex:
            b.warn(f"geom: {ex}")
        if frames is None:
            b.warn("The .geom has no part mounts, parts left in place")
    from mathutils import Matrix

    parts = spec["parts"]
    total = sum(len(p["levels"]) + (1 if p.get("shadow") else 0) for p in parts) or 1
    done = 0
    lod_colls, shadow_coll = {}, None
    roots = {}
    for part in parts:
        pname = part["name"]
        place = frames.get(part.get("role") or "") if frames else None
        world = Matrix(to_blender_matrix(place).tolist()) if place is not None else None
        for level, lv in enumerate(part["levels"]):
            progress(done, total, f"{pname} {level}")
            if level == 0:
                coll = new_collection(bpy, pname, top)
            else:
                if level not in lod_colls:
                    lod_colls[level] = new_collection(bpy, f"LOD{level}", top)
                coll = new_collection(bpy, f"{pname}_lod{level}", lod_colls[level])
            suffix = "" if level == 0 else f".lod{level}"
            root = bpy.data.objects.new(pname + suffix, None)
            root.empty_display_type = "ARROWS"
            root.empty_display_size = 0.1
            root["frost_part"] = pname
            root["frost_level"] = level
            coll.objects.link(root)
            if world is not None:
                root.matrix_world = world
            nodes = []
            for obj in b.objects_of(lv["edf"], lv.get("object")):
                nodes += b.edf_object(lv["edf"], obj, coll, root, suffix)
            b.part_nodes.setdefault(pname, {})[level] = len(nodes)
            add_markers(bpy, b, part, root, coll, nodes, suffix)
            if level == 0:
                roots[part.get("role") or pname] = root
            done += 1
        sh = part.get("shadow")
        if sh:
            progress(done, total, f"{pname} shadow")
            if shadow_coll is None:
                shadow_coll = new_collection(bpy, "Shadow", top)
            coll = new_collection(bpy, f"{pname}_shadow", shadow_coll)
            root = bpy.data.objects.new(f"{pname}.shadow", None)
            root.empty_display_type = "ARROWS"
            root.empty_display_size = 0.1
            root["frost_part"] = pname
            root["frost_shadow"] = True
            coll.objects.link(root)
            if world is not None:
                root.matrix_world = world
            n = 0
            for obj in b.objects_of(sh["edf"], sh.get("object")):
                n += len(b.edf_object(sh["edf"], obj, coll, root, ".shadow"))
            b.part_nodes.setdefault(pname, {})["shadow"] = n
            done += 1
    # Where the game puts the wheels: the axles, as empties.
    if frames:
        for nm, p in axles(spec["geom"]).items():
            e = bpy.data.objects.new(nm, None)
            e.empty_display_type = "CIRCLE"
            e.empty_display_size = 0.1
            top.objects.link(e)
            e.location = (-p[0], -p[2], p[1])
            e.rotation_euler = (0.0, 1.5707963, 0.0)
    # The fork turns with the bars.
    if frames and "fsusp" in roots and "steer" in roots:
        f, s = roots["fsusp"], roots["steer"]
        bpy.context.view_layer.update()
        mw = f.matrix_world.copy()
        f.parent = s
        f.matrix_parent_inverse = s.matrix_world.inverted()
        f.matrix_world = mw
    for c in lod_colls.values():
        hide_layer(bpy, c)
    if shadow_coll is not None:
        hide_layer(bpy, shadow_coll)
    progress(total, total, "save")


def build_fbx_scene(bpy, b, spec):
    progress(0, 1, "fbx")
    before = set(bpy.data.objects)
    bpy.ops.import_scene.fbx(filepath=spec["fbx"])
    top = new_collection(bpy, spec["name"], bpy.context.scene.collection)
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        for c in list(o.users_collection):
            c.objects.unlink(o)
        top.objects.link(o)
    b.part_nodes["fbx"] = {0: len(new)}
    progress(1, 1, "save")


def counts(bpy):
    objs = list(bpy.data.objects)
    return {
        "objects": len(objs),
        "meshes": sum(1 for o in objs if o.type == "MESH"),
        "empties": sum(1 for o in objs if o.type == "EMPTY"),
        "materials": sum(1 for m in bpy.data.materials if m.users),
        "images": sum(1 for i in bpy.data.images if i.users and i.source == "FILE"),
        "triangles": sum(len(o.data.polygons) for o in objs if o.type == "MESH"),
    }


def main():
    import argparse

    import bpy

    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--spec")
    ap.add_argument("--stdin", action="store_true")
    ap.add_argument("--report", required=True)
    a = ap.parse_args(argv)
    if a.stdin:
        spec = read_piped(sys.stdin.buffer)
    else:
        with open(a.spec, "r", encoding="utf-8") as fh:
            spec = json.load(fh)
    t0 = time.time()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    out = spec["out"]
    b = Build(bpy, spec)
    report = {"ok": False}
    try:
        if spec.get("fbx"):
            build_fbx_scene(bpy, b, spec)
        else:
            build_edf_scene(bpy, b, spec)
        bpy.context.scene.view_settings.view_transform = "Standard"
        os.makedirs(os.path.dirname(out), exist_ok=True)
        bpy.ops.wm.save_as_mainfile(filepath=out, relative_remap=True)
        report.update(ok=True, blend=out, **counts(bpy))
    except Exception as ex:
        import traceback

        traceback.print_exc()
        report["error"] = str(ex)
    report["parts"] = b.part_nodes
    report["warnings"] = b.warnings
    report["seconds"] = round(time.time() - t0, 2)
    with open(a.report, "w", encoding="utf-8") as fh:
        json.dump(report, fh)
    print("FI_DONE", json.dumps(report), flush=True)
    if not report["ok"]:
        sys.exit(1)


if __name__ == "__main__":
    main()

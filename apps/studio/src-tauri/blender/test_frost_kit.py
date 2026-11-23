# SPDX-License-Identifier: GPL-3.0-or-later
# frost_kit.py's decisions, with plain Python: `python -m unittest discover -s blender -p "test_*.py"`
# (from apps/studio/src-tauri). `blender_lod_tests_pass` in blender.rs runs it too.

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import frost_kit  # noqa: E402

# The WP 7448 AER Fork's game file, as a customer added it: LOD0 holds 63,905 tris.
WP7448 = {
    "LOD0": [
        ("LOD0_wp_air_screw_L", 570, "steer"),
        ("LOD0_wp_air_valve_L", 170, "steer"),
        ("LOD0_wp_cap_air_L", 5497, "steer"),
        ("LOD0_wp_cap_comp_R", 6970, "steer"),
        ("LOD0_wp_cap_comp_screw_centre_R", 570, "steer"),
        ("LOD0_wp_cap_comp_screw_ring_R", 628, "steer"),
        ("LOD0_wp_fsusp", 11472, "fsusp"),
        ("LOD0_wp_steer", 38028, "steer"),
    ],
}
SUFFIXES = {"LOD1": ("b", 0.6), "LOD2": ("c", 0.4), "SHADOW": ("_shadow", 0.1)}


def wp7448(with_collections=True):
    objs = []
    for level in frost_kit.LEVELS:
        suffix, share = ("", 1.0) if level == "LOD0" else SUFFIXES[level]
        for name, tris, node in WP7448["LOD0"]:
            objs.append({
                "name": name + suffix,
                "collections": ["wp7448_" + level] if with_collections else [],
                "tris": int(tris * share),
                "node": node,
                "parent": None,
            })
    return objs


class PickLod0(unittest.TestCase):
    def test_collections_keep_lod0_only(self):
        keep, report = frost_kit.pick_lod0(wp7448())
        self.assertEqual(keep, {n for n, _, _ in WP7448["LOD0"]})
        self.assertEqual(report["tris"], 63905)
        self.assertEqual(report["leftOut"], ["LOD1", "LOD2", "SHADOW"])
        self.assertEqual(report["by"], "collections")

    def test_names_alone_when_the_collections_are_gone(self):
        # An FBX export of a kit file: no collections, only the kit's name suffixes.
        keep, report = frost_kit.pick_lod0(wp7448(with_collections=False))
        self.assertEqual(keep, {n for n, _, _ in WP7448["LOD0"]})
        self.assertEqual(report["tris"], 63905)
        self.assertEqual(report["by"], "names")

    def test_a_file_without_lods_is_kept_whole(self):
        objs = [{"name": n, "collections": ["Collection"], "tris": 10} for n in ("frame", "fork_l", "fork_r")]
        keep, report = frost_kit.pick_lod0(objs)
        self.assertEqual(keep, {"frame", "fork_l", "fork_r"})
        self.assertIsNone(report)

    def test_one_lookalike_pair_is_not_a_lod_set(self):
        # "pin" and "pinb" alone: one level found, not enough to call it LODs.
        keep, report = frost_kit.pick_lod0([{"name": "pin", "tris": 1}, {"name": "pinb", "tris": 1}])
        self.assertEqual(keep, {"pin", "pinb"})
        self.assertIsNone(report)

    def test_objects_outside_the_lod_collections_stay(self):
        objs = wp7448() + [{"name": "steer_axis", "collections": ["Scene Collection"], "tris": 0}]
        keep, _ = frost_kit.pick_lod0(objs)
        self.assertIn("steer_axis", keep)

    def test_collection_levels(self):
        self.assertEqual(frost_kit.collection_level("wp7448_LOD0"), "LOD0")
        self.assertEqual(frost_kit.collection_level("seat_SHADOW"), "SHADOW")
        self.assertEqual(frost_kit.collection_level("LOD2"), "LOD2")
        self.assertIsNone(frost_kit.collection_level("Collection"))
        self.assertIsNone(frost_kit.collection_level("LOD0_parts"))


class GroupObjects(unittest.TestCase):
    def lod0(self):
        keep, _ = frost_kit.pick_lod0(wp7448())
        return [o for o in wp7448() if o["name"] in keep]

    def test_the_kits_node_decides(self):
        groups = frost_kit.group_objects(self.lod0())
        self.assertEqual(groups["LOD0_wp_fsusp"], "fsusp")
        for name in ("LOD0_wp_steer", "LOD0_wp_cap_air_L", "LOD0_wp_air_valve_L", "LOD0_wp_cap_comp_screw_ring_R"):
            self.assertEqual(groups[name], "steer", name)
        tris = {}
        for o in self.lod0():
            tris[groups[o["name"]]] = tris.get(groups[o["name"]], 0) + o["tris"]
        self.assertEqual(tris, {"steer": 52433, "fsusp": 11472})

    def test_without_nodes_a_piece_follows_its_parent(self):
        objs = [
            {"name": "fork_l", "parent": None},
            {"name": "cap", "parent": "fork_l"},
            {"name": "Cube", "parent": None},
        ]
        groups = frost_kit.group_objects(objs)
        self.assertEqual(groups, {"fork_l": "fsusp", "cap": "fsusp", "Cube": None})

    def test_a_kit_piece_without_a_node_stays_unplaced(self):
        # An FBX export of a kit file loses `mxb_node`: a cap named for neither node isn't
        # guessed onto one (the file then comes in whole, and "Split into parts" keeps it apart).
        objs = [{"name": "LOD0_wp_steer"}, {"name": "LOD0_wp_fsusp"}, {"name": "LOD0_wp_cap_air_L"}]
        groups = frost_kit.group_objects(objs)
        self.assertEqual(groups, {"LOD0_wp_steer": "steer", "LOD0_wp_fsusp": "fsusp", "LOD0_wp_cap_air_L": None})



class SplitOneMesh(unittest.TestCase):
    def test_material_names_become_file_safe_tags(self):
        self.assertEqual(frost_kit.safe_tag("Chrome Tube.001"), "Chrome_Tube")
        self.assertEqual(frost_kit.safe_tag("a/b\c"), "a_b_c")
        self.assertEqual(frost_kit.safe_tag(""), "part")

    def test_repeated_names_are_numbered_and_lone_ones_stay_plain(self):
        self.assertEqual(
            frost_kit.unique_tags(["Chrome", "Anodised", "chrome", "Rubber"]),
            ["Chrome-1", "Anodised", "chrome-2", "Rubber"],
        )

    def test_largest_pieces_keep_their_own_part(self):
        pieces = [("p%d" % i, 100 - i) for i in range(30)]
        own, rest = frost_kit.keep_largest(pieces, limit=24)
        self.assertEqual(len(own), 23)
        self.assertEqual(len(rest), 7)
        self.assertEqual(own[0], 0)
        self.assertEqual(frost_kit.keep_largest(pieces[:5], limit=24), ([0, 1, 2, 3, 4], []))


class GameMaps(unittest.TestCase):
    # The WP 7448's view material, as wp7448_game.blend wires it: the colour sheet through a
    # Mix whose mask is the spec map through Map Range, the spec map through Map Range into
    # Metallic and Roughness, the normal map through a Normal Map node.
    LINKS = [
        ("Principled BSDF", "Material Output", "Surface"),
        ("Image Texture.001", "Map Range", "Value"),
        ("Map Range", "Principled BSDF", "Metallic"),
        ("Image Texture.001", "Map Range.001", "Value"),
        ("Map Range.001", "Principled BSDF", "Roughness"),
        ("Image Texture.001", "Map Range.002", "Value"),
        ("Map Range.002", "Mix", "Factor"),
        ("Image Texture", "Mix", "A"),
        ("Mix", "Principled BSDF", "Base Color"),
        ("Image Texture.002", "Normal Map", "Color"),
        ("Normal Map", "Principled BSDF", "Normal"),
    ]
    IMAGES = {"Image Texture", "Image Texture.001", "Image Texture.002"}

    def test_frost_blend_view_material_gives_all_three_maps(self):
        self.assertEqual(
            frost_kit.game_maps(self.LINKS, self.IMAGES, "Principled BSDF"),
            {"colour": "Image Texture", "spec": "Image Texture.001", "normal": "Image Texture.002"},
        )

    def test_plain_material_wired_straight_in(self):
        links = [("Image Texture", "Principled BSDF", "Base Color")]
        self.assertEqual(
            frost_kit.game_maps(links, {"Image Texture"}, "Principled BSDF"),
            {"colour": "Image Texture", "spec": None, "normal": None},
        )

    def test_untextured_material_has_no_maps(self):
        self.assertEqual(
            frost_kit.game_maps([], set(), "Principled BSDF"),
            {"colour": None, "spec": None, "normal": None},
        )


if __name__ == "__main__":
    unittest.main()

# MXB Cinema replay JSON (v1)

Units: metres, seconds; steer in radians. Game axes kept (left-handed: X right, Y up, Z forward). Convert with `to_blender` = (-x, -z, y), the SAME mapping mxb-blender-kit uses for EDF tracks/bikes, so track and bike line up. `rot` columns are the bike local axes in world (col 2 = forward, col 1 = up); Blender rotation = M @ rot @ M^T (det +1).
```json
{
  "version": 1,
  "source": "rpy | live-capture",
  "track": "folder name under tracks.pkz / mods/tracks",
  "fps": 60,
  "axes": "game",                      // game: X right, Y up, Z forward
  "to_blender": [[-1,0,0],[0,0,-1],[0,1,0]], // blender_vec = M @ game_vec, i.e. (-x,-z,y), same as mxb-blender-kit EDF import; blender_rot = M @ rot @ M^T
  "riders": [{
    "id": 0, "name": "Frost | mxbscre",
    "bike": "MX2OEM_2023_KTM_250_SX-F", "rider_model": "...", "number": 0,
    "frames": [{
      "t": 0.000,                      // seconds from clip start
      "pos": [x, y, z],                // chassis origin, world
      "rot": [[r00,r01,r02],[r10,r11,r12],[r20,r21,r22]], // chassis world rotation, row-major, game axes
      "steer": 0.0,                    // handlebar angle, rad, game sign: + = LEFT, - = right (m_fSteer)
      "fork": 0.0,                     // front susp compression from full extension, m
      "shock": 0.0,                    // rear susp compression, m
      "swingarm": 0.0,                 // swingarm angle, rad (derived from shock if absent)
      "wheel_rot": [0.0, 0.0],         // front/rear accumulated spin angle, rad
      "throttle": 0.0, "brake": [0.0, 0.0], "crashed": false,
      "bones": null                    // optional: {"riderRIG_*": [qw,qx,qy,qz, x,y,z], ...} the game's skeleton, rotation + position in the bike body frame (game axes, not parent-relative); riders[].skeleton has names/parents/bike_offset
    }]
  }],
  "camera": null                       // optional: {"frames":[{"t","pos","rot","fov_deg"}]}
}
```
Missing fields are omitted or null, never faked. Rig: drive the chassis empty from pos/rot, child pivots from steer/fork/swingarm/wheel_rot.

Real data now: `lumberyard_jump.json` (Lumberyard, MX2OEM_2023_KTM_250_SX-F, 5 s at 50 Hz, includes a jump), made by `mxbc2json.py` from MXB Coach's live telemetry (`Documents\PiBoSo\MX Bikes\mxbcoach\sessions\*.mxbc`). Extra per-frame fields: `airborne`, `speed` (m/s). `fork`/`shock` = max travel minus current length (m); `shock` is shock stroke, not wheel travel; `swingarm` is null (derive from shock with the bike's linkage). `wheel_rot` = integral of wheel speed / radius (`wheel_radius` at top level). `bones` null in these files: they were recorded before the recorder had the rider's pose (see below).

## Integration contract (integrate/animate.py)
`animate.py` keys an object named `CINEMA_bike_root` (override with `--root`): location + quaternion rotation in Blender space (EDF import axes; bike forward = Blender -Y), and keyed custom props `steer` (rad), `fork` (m), `shock` (m), `wheel_rot_f`, `wheel_rot_r` (rad). The rig should parent the bike (and rider) under that object at the game chassis origin and drive its pivots with drivers on those props. If the rig's root has another name, tell me and I pass `--root`.

## Rider pose in .mxbc (RIDER_SKELETON 18 / RIDER_BONES 19)
`mxbcoach.dlo` from FrostMod branch `feat/rider-bones-record` (MX Bikes beta21e only) records the local rider's real skeleton as the game drew it, next to each 50 Hz SAMPLE. Full layout, how it was found and a Python decoder: FrostMod `docs/RIDER_BONES.md`; layout pinned in `src/riderbones.h`. Older readers skip both tags by length.
- `RIDER_SKELETON` (18): once per stint. `u8 layout(1), u8 status (0 ok, 1 unknown game build, 2 not found), u8 N nodes, u8 M recorded, u32 exe stamp, f32[3] bike_offset`, then N x `{u8 parent (255 root), u8 recorded, u8 len, char name[len]}`. Node index = position in this list. Names are the game's own (`riderRIG_Head`, `riderRIG_LeftKnee`, ...), the same names `rider_rig` uses. Status != 0 means no bones for that stint: fall back to `rider/pose.py`'s guess, never fake.
- `RIDER_BONES` (19): `f32 t, f32 lap_pos, u8 layout(1), u8 M, u8 flags (bit0 repeat), u8 0, f32[3] origin`, then M x `{u8 node, i16 qx,qy,qz,qw (/32767), i16 x,y,z (mm from origin)}`. 40 nodes on the stock rider (every `riderRIG_*` except fingers), Head included.
- Frame: the bike body as drawn, GAME axes (x right, y up, z forward), origin = chassis origin moved by `bike_offset`. Bone pose in the chassis frame = `T(bike_offset) * T(origin + p) * R(q)`; the quaternion is the bone's rotation in that frame (not parent-relative). To Blender: position `M @ v`, rotation `M @ R @ M^T` with the same `to_blender` M as the bike. `mxbc2json.py` writes each SAMPLE's record into `frames[].bones` as `[qw,qx,qy,qz,x,y,z]` (bike frame, position already includes origin and `bike_offset`), plus `bones_repeat`, and the skeleton into `riders[0].skeleton`; `rider/pose.py` then poses the rig from it (skin = N x bind^-1).
- Unverified until a live game run: the offsets are read statically from beta21e; if a session's skeleton status is 1 or 2, there is nothing to use.

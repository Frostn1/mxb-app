//! A slice of a ride, written as MXB Cinema replay JSON v1 (`blender/SCHEMA.md`).
//!
//! The port of the prototype's `mxbc2json.py`: every sample of the slice as the recorder
//! wrote it, chassis position and matrix in game axes, steer in radians, suspension as
//! compression from full extension, wheel spin integrated from wheel speed. Nothing the
//! recording doesn't hold is invented: the swingarm is left for the rig to derive from the
//! shock. The rider's bones are the game's own where the recorder had them (`RIDER_SKELETON` /
//! `RIDER_BONES`), null where it didn't.

use serde_json::{json, Value};

use crate::mxbc::Recording;

/// Loaded radius of each wheel, front then rear, metres. The game never reports it; these
/// only set how fast the wheels look going round.
pub const WHEEL_RADIUS: [f32; 2] = [0.36, 0.35];

fn r(v: f32, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (v as f64 * k).round() / k
}

fn skeleton(k: &crate::mxbc::Skeleton) -> Value {
    let status = match k.status {
        0 => "ok".to_string(),
        1 => "unknown game build".to_string(),
        2 => "not found".to_string(),
        n => n.to_string(),
    };
    let nodes: Vec<Value> = k.nodes.iter().map(|n| json!({ "name": n.name, "parent": n.parent, "recorded": n.recorded })).collect();
    json!({
        "status": status,
        "exe_stamp": format!("{:#x}", k.exe_stamp),
        "bike_offset": k.bike_offset.iter().map(|&v| v as f64).collect::<Vec<_>>(),
        "nodes": nodes,
        "frame": "bike body (chassis + bike_offset)",
        "axes": "game",
        "bone": "[qw,qx,qy,qz,x,y,z]: rotation and position in the bike frame, not parent-relative",
    })
}

/// The samples between `start` and `end`, seconds from the recording's first sample.
/// `mark` is the moment the shot is about, in the same seconds; it becomes `mark_frame`.
pub fn slice(rec: &Recording, src_file: &str, start: f32, end: f32, mark: f32) -> Result<Value, String> {
    let s = &rec.samples;
    let Some(first) = s.first() else { return Err("empty".into()) };
    let base = first.t;
    let picked: Vec<usize> = (0..s.len()).filter(|&k| s[k].t - base >= start - 1e-4 && s[k].t - base <= end + 1e-4).collect();
    if picked.len() < 2 {
        return Err("tooShort".into());
    }
    let t0 = s[picked[0]].t;
    let travel = rec.event.susp_max_travel;
    let mut spin = [0.0f32; 2];
    let mut prev = t0;
    let mut frames = Vec::with_capacity(picked.len());
    let mut mark_frame = 1usize;
    let mut best = f32::MAX;
    for (n, &k) in picked.iter().enumerate() {
        let x = &s[k];
        let dt = x.t - prev;
        prev = x.t;
        for j in 0..2 {
            spin[j] += x.wheel_speed[j] * dt / WHEEL_RADIUS[j];
        }
        let d = (x.t - base - mark).abs();
        if d < best {
            best = d;
            mark_frame = n + 1;
        }
        let rot: Vec<Vec<f64>> = (0..3).map(|i| (0..3).map(|j| r(x.rot[3 * i + j], 6)).collect()).collect();
        let (bones, repeat) = match rec.bones_at(x.t) {
            Some(b) if b.bones.is_empty() => (Value::Null, b.repeat),
            Some(b) => (Value::Object(b.bones.into_iter().map(|(n, v)| (n, json!(v))).collect()), b.repeat),
            None => (Value::Null, false),
        };
        let lean: Vec<Value> = x.lean.iter().map(|&v| if v.is_finite() { json!(r(v.clamp(-1.0, 1.0), 3)) } else { Value::Null }).collect();
        frames.push(json!({
            "t": r(x.t - t0, 4),
            "pos": [r(x.pos[0], 4), r(x.pos[1], 4), r(x.pos[2], 4)],
            "rot": rot,
            "steer": r(x.steer.to_radians(), 5),
            "fork": r((travel[0] - x.susp[0]).max(0.0), 4),
            "shock": r((travel[1] - x.susp[1]).max(0.0), 4),
            "swingarm": Value::Null,
            "wheel_rot": [r(spin[0], 4), r(spin[1], 4)],
            "throttle": r(x.throttle, 3),
            "brake": [r(x.brake[0], 3), r(x.brake[1], 3)],
            "crashed": x.crashed,
            "airborne": x.air,
            "speed": r(x.speed, 3),
            "roll_deg": r(x.roll, 2),
            "lean": lean,
            "stance": match x.stance { 1 => "stand", 2 => "sit", _ => "unknown" },
            "bones": bones,
            "bones_repeat": repeat,
        }));
    }
    let e = &rec.event;
    Ok(json!({
        "version": 1,
        "source": "live-capture (mxbcoach .mxbc, SPluginsBikeData_t)",
        "src_file": src_file,
        "src_t0": r(t0 - base, 3),
        "track": e.track_id,
        "fps": 50,
        "axes": "game",
        "to_blender": [[-1, 0, 0], [0, 0, -1], [0, 1, 0]],
        "susp_max_travel": [travel[0], travel[1]],
        "wheel_radius": WHEEL_RADIUS,
        "mark_frame": mark_frame,
        "riders": [{
            "id": 0,
            "name": e.rider,
            "bike": e.bike_id,
            "rider_model": Value::Null,
            "number": Value::Null,
            "skeleton": rec.skeleton.as_ref().map(skeleton).unwrap_or(Value::Null),
            "frames": frames,
        }],
        "camera": Value::Null,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxbc::{self, test_file};

    #[test]
    fn bones_and_skeleton_in_the_clip() {
        let rec = mxbc::parse(&test_file(0, true)).unwrap();
        let v = slice(&rec, "x.mxbc", 0.0, 1.0, 0.02).unwrap();
        let r = &v["riders"][0];
        assert_eq!(r["skeleton"]["status"], "ok");
        assert_eq!(r["skeleton"]["exe_stamp"], "0x6a21833d");
        assert_eq!(r["skeleton"]["nodes"][0]["parent"], Value::Null);
        assert_eq!(r["skeleton"]["nodes"][2]["parent"], 1);
        let f = r["frames"].as_array().unwrap();
        assert_eq!(f.len(), 3);
        let head = &f[0]["bones"]["riderRIG_Head"];
        assert_eq!(head.as_array().unwrap().len(), 7);
        assert!((head[0].as_f64().unwrap() - 1.0).abs() < 1e-9);
        // bike offset 0.1 + origin 0.5 + 250 mm
        assert!((head[4].as_f64().unwrap() - 0.85).abs() < 1e-4);
        assert_eq!(f[0]["bones_repeat"], false);
        assert_eq!(f[1]["bones_repeat"], true);
        // the third sample has no bones record
        assert_eq!(f[2]["bones"], Value::Null);
        assert_eq!(f[2]["bones_repeat"], false);
    }

    #[test]
    fn no_bones_without_the_records() {
        let rec = mxbc::parse(&test_file(0, false)).unwrap();
        let v = slice(&rec, "x.mxbc", 0.0, 1.0, 0.0).unwrap();
        assert_eq!(v["riders"][0]["skeleton"], Value::Null);
        assert_eq!(v["riders"][0]["frames"][0]["bones"], Value::Null);
    }
}

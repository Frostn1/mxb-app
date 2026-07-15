// TEMP end-to-end check against real sample .pkz files. Ignored by default.
use std::path::Path;

#[path = "../src/pkz.rs"]
mod pkz;

#[test]
#[ignore]
fn real_samples() {
    let cases = [
        "/Users/seandahan/Downloads/pkz/old-maps/FLRMX.pkz",
        "/Users/seandahan/Downloads/pkz/old-maps/RED - Practice Track.pkz",
        "/Users/seandahan/Downloads/pkz/old-maps/NXT LVL 101.pkz",
        "/Users/seandahan/Downloads/SandPointMX.pkz",
        "/Users/seandahan/Downloads/JVxHM_Hawkstone_Pro.pkz",
    ];
    for c in cases {
        let p = Path::new(c);
        if !p.exists() { println!("SKIP (missing): {c}"); continue; }
        let m = pkz::read_meta(p).expect("read_meta");
        println!(
            "{}\n  locked={} name={:?} author={:?} len={:?} alt={:?} thumb={}",
            c, m.locked, m.name, m.author, m.length, m.altitude,
            m.thumbnail.as_ref().map(|t| format!("{} chars b64", t.len())).unwrap_or_else(|| "none".into())
        );
    }
}

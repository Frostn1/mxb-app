use mxb_content::{beta21e_trh_manifest_checks, TrackPackage};
use std::path::PathBuf;

fn main() {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: cargo run -p mxb-content --example inspect_track -- TRACK.pkz");
        std::process::exit(2);
    };
    match TrackPackage::open(&path) {
        Ok(track) => {
            let (blocks_x, blocks_y) = track.world_block_grid();
            let rdf = match track.rdf_bootstrap() {
                Ok(rdf) => rdf,
                Err(error) => {
                    eprintln!("{}: {error:#}", path.display());
                    std::process::exit(1);
                }
            };
            println!("id={}", track.id);
            println!("terrain={}x{}", track.terrain.width, track.terrain.height);
            println!("world_blocks={}x{}", blocks_x, blocks_y);
            println!(
                "stalls=pit:{} board:{} grid:{}",
                rdf.pit_lane.start_stalls.len(),
                rdf.pit_board.stalls.len(),
                rdf.starting_grid.stalls.len()
            );
            println!(
                "30seconds_board={}/{}/{}",
                rdf.thirty_seconds_board.long,
                rdf.thirty_seconds_board.lat,
                rdf.thirty_seconds_board.angle
            );
            println!("ini={}", track.ini_entry);
            println!("rdf={}", track.rdf_entry);
            println!("trh={}", track.trh_entry);
            let terrain = track
                .read_selected(256 * 1024 * 1024, |name| {
                    name.eq_ignore_ascii_case(&track.trh_entry)
                })
                .expect("read selected TRH")
                .pop()
                .expect("selected TRH")
                .1;
            let checks =
                beta21e_trh_manifest_checks(&terrain, None).expect("parse TRH manifest checks");
            println!(
                "trh_manifest={:08x}/{:08x}/{:08x}",
                checks.canonical_byte_count,
                checks
                    .canonical_byte_sum
                    .expect("beta21e canonical byte sum"),
                checks.auxiliary_byte_sum
            );
        }
        Err(error) => {
            eprintln!("{}: {error:#}", path.display());
            std::process::exit(1);
        }
    }
}

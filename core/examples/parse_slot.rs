//! Parse a `slot_data` JSON file with the app's own reader: `cargo run --example parse_slot -- path.json`.
fn main() {
    let path = std::env::args().nth(1).expect("path to a slot_data json file");
    let text = std::fs::read_to_string(path).expect("read file");
    match apgo_core::slot::SlotData::from_json(&text) {
        Ok(d) => println!("OK schema {} goals {:?} mode {:?} need {} trips {}", d.schema_version, d.goal_list(), d.goal_mode, d.goal_need, d.trips.len()),
        Err(e) => {
            eprintln!("REFUSED: {e}");
            std::process::exit(1);
        }
    }
}

//! The cabinet daemon. It will read scores from the emulator on a RetroPie box
//! and post them to the API. For now it only proves that the crate builds and
//! that the shared models are reachable.

use bunker_models::generate_glyph;

fn main() {
    let glyph = generate_glyph("cabd");

    println!("bunker-cabd {}", env!("CARGO_PKG_VERSION"));
    for row in glyph.rows() {
        println!("{row}");
    }
}

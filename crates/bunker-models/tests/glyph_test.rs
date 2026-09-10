//! The glyph algorithm is shared with the web renderer in `web/src/lib/glyph.ts`.
//! The vectors below come from that implementation. A change that breaks them
//! gives a player a different mark on the site and in the terminal.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use bunker_models::{GLYPH_CELLS, GLYPH_SIZE, Glyph, GlyphBits, GlyphColor, generate_glyph};

#[test]
fn known_handles_produce_the_same_marks_as_the_web_renderer() {
    let cases = [
        ("dave", 4_554_623, GlyphColor::Gold),
        ("ziopera", 18_404_923, GlyphColor::Coral),
        ("mortadella", 32_516_768, GlyphColor::Amber),
    ];

    for (handle, bits, color) in cases {
        let glyph = generate_glyph(handle);

        assert_eq!(
            glyph.bits.into_inner(),
            bits,
            "{handle} produced other bits"
        );
        assert_eq!(glyph.color, color, "{handle} produced another color");
    }
}

#[test]
fn the_seed_is_case_insensitive() {
    assert_eq!(generate_glyph("Dave"), generate_glyph("dave"));
    assert_eq!(generate_glyph("MORTADELLA"), generate_glyph("mortadella"));
}

#[test]
fn every_mark_is_horizontally_symmetric() {
    for seed in [
        "a",
        "bb",
        "ciccio",
        "Fede_88",
        "vale_98",
        "gabri.exe",
        "x-y-z",
    ] {
        for row in generate_glyph(seed).rows() {
            let reversed: String = row.chars().rev().collect();
            assert_eq!(row, reversed, "{seed} produced an asymmetric row {row}");
        }
    }
}

#[test]
fn every_mark_has_a_lit_center_column_and_enough_weight() {
    for seed in [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n",
    ] {
        let cells = generate_glyph(seed).cells();

        let center_lit = (0..GLYPH_SIZE).any(|row| cells[row * GLYPH_SIZE + 2]);
        assert!(center_lit, "{seed} has an empty center column");

        let lit = cells.iter().filter(|lit| **lit).count();
        assert!(lit >= 7, "{seed} has only {lit} lit cells");
    }
}

#[test]
fn bits_and_cells_round_trip() {
    let glyph = generate_glyph("dave");
    let cells = glyph.cells();

    let rebuilt =
        cells.iter().enumerate().fold(
            0_u32,
            |bits, (index, lit)| if *lit { bits | (1 << index) } else { bits },
        );

    assert_eq!(rebuilt, glyph.bits.into_inner());
    assert_eq!(cells.len(), GLYPH_CELLS);
}

#[test]
fn rows_render_the_dave_mark() {
    let rows = generate_glyph("dave").rows();

    assert_eq!(rows, ["#####", "##.##", "#####", ".#.#.", "..#.."]);
}

#[test]
fn bits_past_the_grid_are_rejected() {
    assert!(GlyphBits::try_new(1 << 25).is_err());
    assert!(GlyphBits::try_new((1 << 25) - 1).is_ok());
}

#[test]
fn a_glyph_serializes_with_the_hex_color_the_web_uses() {
    let json = serde_json::to_value(generate_glyph("dave")).unwrap();

    assert_eq!(json["bits"], 4_554_623);
    assert_eq!(json["color"], "#ffd75c");

    let parsed: Glyph = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, generate_glyph("dave"));
}

#[test]
fn every_color_round_trips_through_its_hex_value() {
    for color in bunker_models::GLYPH_COLORS {
        assert_eq!(GlyphColor::from_hex(color.hex()), Some(color));
    }
    assert_eq!(GlyphColor::from_hex("#000000"), None);
}

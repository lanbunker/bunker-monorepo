//! The arcade types: a ROM set name and a score.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bunker_models::{GAME_TITLE_MAX_LEN, GameTitle, ROM_NAME_MAX_LEN, RomName, SCORE_MAX, Score};

#[test]
fn a_rom_name_trims_and_accepts_lowercase_digits_and_underscore() {
    assert_eq!(RomName::try_new(" bublbobl ").unwrap().as_ref(), "bublbobl");
    assert!(RomName::try_new("mspacman").is_ok());
    assert!(RomName::try_new("sf2_ce").is_ok());
    assert!(RomName::try_new("1943").is_ok());
}

#[test]
fn a_rom_name_refuses_uppercase_punctuation_and_bad_lengths() {
    assert!(RomName::try_new("Galaga").is_err());
    assert!(RomName::try_new("ms-pacman").is_err());
    assert!(RomName::try_new("ms.pacman").is_err());
    assert!(RomName::try_new("dig dug").is_err());
    assert!(RomName::try_new("").is_err());
    assert!(RomName::try_new("   ").is_err());
    assert!(RomName::try_new("a".repeat(ROM_NAME_MAX_LEN)).is_ok());
    assert!(RomName::try_new("a".repeat(ROM_NAME_MAX_LEN + 1)).is_err());
}

#[test]
fn a_rom_name_is_a_plain_string_on_the_wire() {
    let name: RomName = serde_json::from_str("\"galaga\"").unwrap();
    assert_eq!(name.as_ref(), "galaga");
    assert!(serde_json::from_str::<RomName>("\"Galaga\"").is_err());
    assert_eq!(serde_json::to_string(&name).unwrap(), "\"galaga\"");
}

#[test]
fn a_score_is_a_bounded_plain_number_on_the_wire_and_orders_by_value() {
    let score = Score::try_new(998_000).unwrap();
    assert_eq!(serde_json::to_string(&score).unwrap(), "998000");
    let parsed: Score = serde_json::from_str("1234560").unwrap();
    assert_eq!(parsed.into_inner(), 1_234_560);
    assert!(Score::try_new(10).unwrap() < Score::try_new(11).unwrap());
    assert!(Score::try_new(SCORE_MAX).is_ok());
    assert!(Score::try_new(SCORE_MAX + 1).is_err());
    assert!(serde_json::from_str::<Score>(&(SCORE_MAX + 1).to_string()).is_err());
}

#[test]
fn a_game_title_trims_and_has_length_limits() {
    assert_eq!(
        GameTitle::try_new(" Bubble Bobble ").unwrap().as_ref(),
        "Bubble Bobble"
    );
    assert!(GameTitle::try_new("   ").is_err());
    assert!(GameTitle::try_new("a".repeat(GAME_TITLE_MAX_LEN)).is_ok());
    assert!(GameTitle::try_new("a".repeat(GAME_TITLE_MAX_LEN + 1)).is_err());
}

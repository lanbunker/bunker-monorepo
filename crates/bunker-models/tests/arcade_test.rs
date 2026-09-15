//! The arcade types: a ROM set name and a score.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bunker_models::{ROM_NAME_MAX_LEN, RomName, Score};

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
fn a_score_is_a_plain_number_on_the_wire_and_orders_by_value() {
    assert_eq!(serde_json::to_string(&Score(998_000)).unwrap(), "998000");
    let score: Score = serde_json::from_str("1234560").unwrap();
    assert_eq!(score, Score(1_234_560));
    assert!(Score(10) < Score(11));
}

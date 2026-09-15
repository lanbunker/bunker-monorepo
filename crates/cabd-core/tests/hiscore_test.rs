#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use bunker_models::Score;
use cabd_core::hiscore::{Decoder, HiscoreError, decode};
use cabd_core::view::format_score;

#[test]
fn ascii_decimal_reads_digits_and_ignores_whitespace() {
    assert_eq!(
        decode(Decoder::AsciiDecimal, b"12345").unwrap(),
        Score(12345)
    );
    assert_eq!(decode(Decoder::AsciiDecimal, b"  7\n").unwrap(), Score(7));
}

#[test]
fn ascii_decimal_refuses_empty_and_non_digits() {
    assert!(matches!(
        decode(Decoder::AsciiDecimal, b"   "),
        Err(HiscoreError::Empty)
    ));
    assert!(matches!(
        decode(Decoder::AsciiDecimal, b"12a"),
        Err(HiscoreError::NotDecimal)
    ));
    assert!(matches!(
        decode(Decoder::AsciiDecimal, &[0xff, 0x00]),
        Err(HiscoreError::NotDecimal)
    ));
    assert!(matches!(
        decode(Decoder::AsciiDecimal, b"99999999999999999999999"),
        Err(HiscoreError::Overflow)
    ));
}

#[test]
fn format_score_groups_three_digits() {
    assert_eq!(format_score(Score(0)), "0");
    assert_eq!(format_score(Score(999)), "999");
    assert_eq!(format_score(Score(1000)), "1,000");
    assert_eq!(format_score(Score(1_234_560)), "1,234,560");
}

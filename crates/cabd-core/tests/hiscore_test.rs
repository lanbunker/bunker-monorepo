#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bunker_models::{SCORE_MAX, Score};
use cabd_core::hiscore::{Decoder, HiscoreError, decode};
use cabd_core::view::format_score;

fn score(v: u64) -> Score {
    Score::try_new(v).unwrap()
}

#[test]
fn ascii_decimal_reads_digits_and_ignores_whitespace() {
    assert_eq!(
        decode(Decoder::AsciiDecimal, b"12345").unwrap(),
        score(12345)
    );
    assert_eq!(decode(Decoder::AsciiDecimal, b"  7\n").unwrap(), score(7));
}

#[test]
fn ascii_decimal_refuses_empty_non_digits_and_too_large() {
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
    assert_eq!(
        decode(Decoder::AsciiDecimal, SCORE_MAX.to_string().as_bytes()).unwrap(),
        score(SCORE_MAX)
    );
    assert!(matches!(
        decode(
            Decoder::AsciiDecimal,
            (SCORE_MAX + 1).to_string().as_bytes()
        ),
        Err(HiscoreError::TooLarge)
    ));
    assert!(matches!(
        decode(Decoder::AsciiDecimal, b"99999999999999999999999"),
        Err(HiscoreError::TooLarge)
    ));
}

#[test]
fn format_score_groups_three_digits() {
    assert_eq!(format_score(score(0)), "0");
    assert_eq!(format_score(score(999)), "999");
    assert_eq!(format_score(score(1000)), "1,000");
    assert_eq!(format_score(score(1_234_560)), "1,234,560");
    assert_eq!(format_score(score(SCORE_MAX)), "999,999,999,999");
}

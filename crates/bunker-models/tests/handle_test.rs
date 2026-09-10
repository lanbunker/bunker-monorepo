//! The domain types refuse invalid values, so no module below them checks again.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use bunker_models::{
    HANDLE_MAX_LEN, HANDLE_MIN_LEN, Handle, HandleError, PASSWORD_MAX_LEN, PASSWORD_MIN_LEN,
    Password, PasswordError, PlayerId, SignupRequest,
};

#[test]
fn a_handle_trims_and_keeps_its_case() {
    let handle = Handle::try_new("  Fede_88 ").unwrap();

    assert_eq!(handle.as_ref(), "Fede_88");
}

#[test]
fn a_handle_accepts_letters_digits_underscore_dot_and_dash() {
    for raw in ["dave", "gabri.exe", "vale_98", "x-ray", "ABC"] {
        assert!(Handle::try_new(raw).is_ok(), "{raw} was rejected");
    }
}

#[test]
fn a_handle_rejects_spaces_and_symbols() {
    for raw in ["da ve", "dave!", "dave/root", "dàve", "😀😀😀"] {
        assert!(
            matches!(Handle::try_new(raw), Err(HandleError::PredicateViolated)),
            "{raw} was accepted"
        );
    }
}

#[test]
fn a_handle_has_length_limits() {
    assert!(matches!(
        Handle::try_new("a".repeat(HANDLE_MIN_LEN - 1)),
        Err(HandleError::LenCharMinViolated)
    ));
    assert!(matches!(
        Handle::try_new("a".repeat(HANDLE_MAX_LEN + 1)),
        Err(HandleError::LenCharMaxViolated)
    ));
    assert!(Handle::try_new("a".repeat(HANDLE_MIN_LEN)).is_ok());
    assert!(Handle::try_new("a".repeat(HANDLE_MAX_LEN)).is_ok());
}

#[test]
fn a_password_has_length_limits_and_is_not_trimmed() {
    assert!(matches!(
        Password::try_new("a".repeat(PASSWORD_MIN_LEN - 1)),
        Err(PasswordError::LenCharMinViolated)
    ));
    assert!(matches!(
        Password::try_new("a".repeat(PASSWORD_MAX_LEN + 1)),
        Err(PasswordError::LenCharMaxViolated)
    ));

    let spaced = Password::try_new("  spaces count  ").unwrap();
    assert_eq!(spaced.as_ref(), "  spaces count  ");
}

#[test]
fn a_password_never_prints_itself() {
    let password = Password::try_new("hunter2hunter2").unwrap();

    assert_eq!(format!("{password:?}"), "Password(<redacted>)");
}

#[test]
fn a_signup_body_runs_the_same_validation_and_rejects_unknown_fields() {
    let ok: SignupRequest =
        serde_json::from_str(r#"{"handle":"dave","password":"hunter2hunter2"}"#).unwrap();
    assert_eq!(ok.handle.as_ref(), "dave");

    assert!(
        serde_json::from_str::<SignupRequest>(r#"{"handle":"d","password":"hunter2hunter2"}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<SignupRequest>(r#"{"handle":"dave","password":"short"}"#).is_err()
    );
    assert!(
        serde_json::from_str::<SignupRequest>(
            r#"{"handle":"dave","password":"hunter2hunter2","role":"admin"}"#
        )
        .is_err()
    );
}

#[test]
fn player_ids_are_distinct() {
    assert_ne!(PlayerId::generate(), PlayerId::generate());
}

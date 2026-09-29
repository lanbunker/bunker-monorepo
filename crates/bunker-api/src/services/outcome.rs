/// What an idempotent write did: it made the row, or the row was already there.
/// A router answers `201` for the first and `200` for the second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<T> {
    Created(T),
    Existing(T),
}

/// Who a list or a detail is for. Only an admin sees a draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    WithDrafts,
}

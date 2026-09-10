//! The page window arithmetic that every list route shares.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

#[test]
fn pagination_translates_to_limit_and_offset_and_rounds_pages_up() {
    use bunker_models::{PAGE_SIZE_MAX, PageNumber, PageQuery, PageSize, Paginated};

    let query = PageQuery {
        page: PageNumber::try_new(3).unwrap(),
        page_size: PageSize::try_new(25).unwrap(),
    };
    assert_eq!((query.limit(), query.offset()), (25, 50));
    assert_eq!(PageQuery::default().offset(), 0);

    let pages_for =
        |total| Paginated::new(Vec::<u8>::new(), total, PageQuery::default()).total_pages;
    assert_eq!(
        (pages_for(0), pages_for(1), pages_for(20), pages_for(21)),
        (0, 1, 1, 2)
    );

    assert!(PageNumber::try_new(0).is_err());
    assert!(PageSize::try_new(PAGE_SIZE_MAX + 1).is_err());
    assert!(serde_json::from_str::<PageQuery>(r#"{"page_size":5}"#).is_err());
}

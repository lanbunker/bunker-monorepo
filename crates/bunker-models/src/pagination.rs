use nutype::nutype;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::handle::HANDLE_MAX_LEN;

/// The largest page a client can ask for.
pub const PAGE_SIZE_MAX: u32 = 100;

/// A 1-based page index.
#[nutype(
    validate(greater_or_equal = 1),
    default = 1,
    derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)
)]
pub struct PageNumber(u32);

/// The number of items in one page. The maximum is [`PAGE_SIZE_MAX`].
#[nutype(
    validate(greater_or_equal = 1, less_or_equal = PAGE_SIZE_MAX),
    default = 20,
    derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)
)]
pub struct PageSize(u32);

/// The query string of every list route. An unknown key is an error: a server
/// that ignored `?page_size=50` would answer page 1 of 20 and look correct.
#[derive(Debug, Clone, Copy, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    #[serde(default)]
    #[param(value_type = u32, minimum = 1, default = 1)]
    pub page: PageNumber,
    #[serde(default)]
    #[param(value_type = u32, minimum = 1, maximum = 100, default = 20)]
    pub page_size: PageSize,
}

impl PageQuery {
    pub fn limit(self) -> i64 {
        i64::from(self.page_size.into_inner())
    }

    pub fn offset(self) -> i64 {
        (i64::from(self.page.into_inner()) - 1) * i64::from(self.page_size.into_inner())
    }
}

/// A piece of a handle to look for, matched without case. A term longer than a
/// handle can match nothing, so it is refused like an empty one.
#[nutype(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = HANDLE_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Deserialize)
)]
pub struct SearchTerm(String);

/// The query string of a player list: a page and an optional search. It is
/// not [`PageQuery`] with a field added, because `deny_unknown_fields` does not
/// work through `serde(flatten)`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct RosterQuery {
    #[serde(default)]
    #[param(value_type = u32, minimum = 1, default = 1)]
    pub page: PageNumber,
    #[serde(default)]
    #[param(value_type = u32, minimum = 1, maximum = 100, default = 20)]
    pub page_size: PageSize,
    /// A piece of a handle. The match ignores case.
    #[param(value_type = Option<String>, min_length = 1, max_length = 20)]
    pub q: Option<SearchTerm>,
}

impl RosterQuery {
    pub fn page(&self) -> PageQuery {
        PageQuery {
            page: self.page,
            page_size: self.page_size,
        }
    }

    pub fn term(&self) -> Option<&str> {
        self.q.as_ref().map(AsRef::as_ref)
    }
}

/// One page of results, and the totals a client needs for a pager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Paginated<T> {
    pub items: Vec<T>,
    /// Every row that matches, ignoring the window.
    pub total: u64,
    #[schema(value_type = u32, minimum = 1)]
    pub page: PageNumber,
    #[schema(value_type = u32, minimum = 1, maximum = 100)]
    pub page_size: PageSize,
    pub total_pages: u64,
}

impl<T> Paginated<T> {
    pub fn new(items: Vec<T>, total: u64, query: PageQuery) -> Self {
        Self {
            items,
            total,
            page: query.page,
            page_size: query.page_size,
            total_pages: total.div_ceil(u64::from(query.page_size.into_inner())),
        }
    }
}

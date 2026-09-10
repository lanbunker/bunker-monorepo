use nutype::nutype;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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

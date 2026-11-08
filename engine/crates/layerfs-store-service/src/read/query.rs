//! Stable numbered result pages and explicit per-request replay budgets.
//! A page counts matching results, never candidates silently filtered away.
use crate::history::lookup::{invalid, refuse};
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// Maximum candidates evaluated by one numbered query, including seeking.
/// This bounds one request's work, not the size of a Store or its history.
pub const MAXIMUM_QUERY_WORK: u64 = 10_000;

/// A checked, one-based page of matching results.
#[derive(Clone, Copy, Debug)]
pub struct ResultPage {
    /// Maximum matching items to return.
    pub limit: usize,
    /// Matching items preceding this page.
    pub skip: u64,
    /// Public one-based page number.
    pub number: u64,
}

/// A numbered query's page. Legacy offset/cursor options cannot be combined.
pub fn result_page(args: &SdkValue) -> Result<ResultPage, SdkRefusal> {
    for name in ["pagePos", "cursor"] {
        if args.get(name).is_some_and(|v| !matches!(v, SdkValue::Null)) {
            return Err(invalid("page cannot be combined with page_pos or cursor"));
        }
    }
    let positive = |name: &str, default: u64| match args.get(name) {
        None | Some(SdkValue::Null) => Ok(default),
        Some(SdkValue::Int(n)) if (1..=9_007_199_254_740_991).contains(n) => Ok(*n as u64),
        _ => Err(invalid(format!("{name} must be a positive safe integer"))),
    };
    let number = positive("page", 1)?;
    let size = positive("pageSize", 50)?;
    if size > 1000 {
        return Err(invalid("page_size must be in 1..=1000"));
    }
    let skip = (number - 1)
        .checked_mul(size)
        .ok_or_else(|| invalid("page offset overflows"))?;
    if skip >= MAXIMUM_QUERY_WORK {
        return Err(capacity("numbered page exceeds the query replay budget"));
    }
    Ok(ResultPage {
        limit: size as usize,
        skip,
        number,
    })
}

/// A query cannot complete safely within its declared work/reply budget.
pub fn capacity(message: &str) -> SdkRefusal {
    refuse(
        "Capacity",
        message,
        SdkValue::map([
            ("complete", SdkValue::Bool(false)),
            ("queryWorkLimit", SdkValue::Int(MAXIMUM_QUERY_WORK as i64)),
        ]),
    )
}

/// One request's candidate visits. Exhaustion refuses the entire page so the
/// caller never advances past an evaluated but unreturned matching record.
#[derive(Default)]
pub struct Work {
    visited: u64,
}

impl Work {
    /// Accounts for one candidate before doing its work.
    pub fn step(&mut self) -> Result<(), SdkRefusal> {
        if self.visited == MAXIMUM_QUERY_WORK {
            return Err(capacity(
                "query candidate budget exhausted; page is incomplete",
            ));
        }
        self.visited += 1;
        Ok(())
    }

    /// Candidates examined by this request.
    pub fn examined(&self) -> u64 {
        self.visited
    }
}

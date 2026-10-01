//! Independent ordered table folds and explicit terminal continuation checks.
use crate::filesystem::state::{
    FactLedger, FactSeal, FactState, ParentEligibilityState, ParentSeal,
};
use crate::{ContentError, ContentResult};
fn progress(
    seal: &FactSeal,
    after: Option<u64>,
    last: Option<u64>,
    eof: bool,
    count: usize,
    total: u64,
) -> ContentResult<()> {
    if total > seal.records || eof != (total == seal.records) || (count != 0 && last <= after) {
        return Err(ContentError::InvalidOrderingRecord(
            "fact verifier EOF/progress",
        ));
    }
    Ok(())
}
pub(super) fn base<S: FactState + ?Sized>(state: &mut S, seal: &FactSeal) -> ContentResult<()> {
    let _fold = state
        .fact_memory(&seal.scope)?
        .reserve(std::mem::size_of::<FactLedger>())?;
    let mut ledger = FactLedger::new(seal.scope.clone())?;
    let mut after = None;
    loop {
        let page = state.fact_page(seal, after, 128, 65536)?;
        if page.seal != *seal {
            return Err(ContentError::InvalidOrderingRecord("fact verifier seal"));
        }
        ledger.base(page.records())?;
        if page.last != ledger.last().or(after) {
            return Err(ContentError::InvalidOrderingRecord(
                "fact verifier continuation",
            ));
        }
        progress(
            seal,
            after,
            page.last,
            page.eof,
            page.records().len(),
            ledger.records(),
        )?;
        after = page.last;
        if page.eof {
            break;
        }
    }
    if ledger.seal() != *seal {
        return Err(ContentError::InvalidOrderingRecord(
            "fact verifier transcript",
        ));
    }
    Ok(())
}
pub(super) fn parents<S: FactState + ParentEligibilityState + ?Sized>(
    state: &mut S,
    seal: &ParentSeal,
) -> ContentResult<()> {
    let base = crate::filesystem::state::FactScope::new(
        seal.facts.scope.state().selection().clone(),
        seal.facts.scope.subject().clone(),
    )?;
    let _fold = state
        .fact_memory(&base)?
        .reserve(std::mem::size_of::<FactLedger>())?;
    let mut ledger = FactLedger::new(seal.facts.scope.clone())?;
    let mut after = None;
    loop {
        let page = state.parent_page(seal, after, 128, 65536)?;
        if page.seal != seal.facts {
            return Err(ContentError::InvalidOrderingRecord("parent verifier seal"));
        }
        ledger.parents(page.records())?;
        if page.last != ledger.last().or(after) {
            return Err(ContentError::InvalidOrderingRecord(
                "parent verifier continuation",
            ));
        }
        progress(
            &seal.facts,
            after,
            page.last,
            page.eof,
            page.records().len(),
            ledger.records(),
        )?;
        after = page.last;
        if page.eof {
            break;
        }
    }
    if ledger.parent_seal()? != *seal {
        return Err(ContentError::InvalidOrderingRecord(
            "parent verifier transcript",
        ));
    }
    Ok(())
}

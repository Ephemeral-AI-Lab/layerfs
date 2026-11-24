//! The lease heartbeat: one fenced, monotonic extension of an intent's
//! deadline. An expired lease is uncertain, never dead, so renewal is the
//! owning call's way to stay a live holder without rewriting the row.
use super::*;

const RENEW: &str = "gc_intent_renew";

/// Extends one intent's lease to `lease_expires_ms` and answers whether the
/// Store still records it.
///
/// A presented stale fencing token is `Conflict` and leaves the row for the
/// newer owner; a caller that presents none is the unfenced owner the release
/// path has always had. The statement keeps the later deadline, so concurrent
/// or delayed heartbeats never shorten a lease. An absent row is `false`: the
/// owner was already settled and a heartbeat must not resurrect it.
pub(crate) fn renew_lease(
    provider: &HistoryProvider,
    op_id: &[u8; 16],
    fence_token: Option<&str>,
    lease_expires_ms: i64,
) -> HistoryResult<bool> {
    if lease_expires_ms < 0 {
        return Err(HistoryError::InvalidInput("GC lease deadline"));
    }
    provider.write(|tx| {
        let Some(intent) = get_in(tx, op_id)? else {
            return Ok(false);
        };
        super::authority::fence_check(&intent, fence_token)?;
        tx.execute(RENEW, params![lease_expires_ms, now_ms(), op_id.as_slice()])?;
        Ok(true)
    })
}

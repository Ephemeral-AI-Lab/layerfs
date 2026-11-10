//! Stateless saved-query continuations. Only authenticated positions are trusted.
use crate::{history::lookup as args, store::Store};
use layerfs_bridge::control::{Answer, Reply, SdkRefusal, SdkValue};

// Leave room for both the result page and continuation in one control record.
const MAXIMUM_PAYLOAD: usize = 4 * 1024;

pub fn encode(store: &Store, query: SdkValue, state: SdkValue) -> Result<SdkValue, SdkRefusal> {
    let payload = Answer {
        id: 1,
        reply: Reply::Sdk(SdkValue::map([("query", query), ("state", state)])),
    }
    .encode()
    .map_err(|_| super::query::capacity("continuation payload exceeds control budget"))?;
    if payload.len() > MAXIMUM_PAYLOAD {
        return Err(super::query::capacity(
            "continuation payload exceeds query budget",
        ));
    }
    let tag = store
        .history()
        .query_authenticator(&payload)
        .map_err(args::history)?;
    Ok(SdkValue::text(format!(
        "lfq1:{}:{}",
        args::hex(&payload),
        args::hex(&tag)
    )))
}

pub fn decode(store: &Store, token: &str, query: &SdkValue) -> Result<SdkValue, SdkRefusal> {
    let invalid = || args::invalid("continuation does not match this immutable pin and query");
    if token.len() > 2 * MAXIMUM_PAYLOAD + 70 {
        return Err(invalid());
    }
    let (body, tag) = token
        .strip_prefix("lfq1:")
        .and_then(|s| s.split_once(':'))
        .ok_or_else(invalid)?;
    let payload = args::unhex(body).ok_or_else(invalid)?;
    let tag = args::unhex(tag)
        .filter(|t| t.len() == 32)
        .ok_or_else(invalid)?;
    let expected = store
        .history()
        .query_authenticator(&payload)
        .map_err(args::history)?;
    if tag
        .iter()
        .zip(expected)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        != 0
    {
        return Err(invalid());
    }
    let Answer {
        id: 1,
        reply: Reply::Sdk(value),
    } = Answer::decode(&payload).map_err(|_| invalid())?
    else {
        return Err(invalid());
    };
    if value.get("query") != Some(query) {
        return Err(invalid());
    }
    value.get("state").cloned().ok_or_else(invalid)
}

pub fn page(state: &SdkValue, requested: u64) -> Result<u64, SdkRefusal> {
    let next = state
        .get_int("page")
        .filter(|n| *n > 1)
        .ok_or_else(|| args::invalid("invalid continuation page"))? as u64;
    if requested != 1 && requested != next {
        return Err(args::invalid("page does not match continuation"));
    }
    Ok(next)
}

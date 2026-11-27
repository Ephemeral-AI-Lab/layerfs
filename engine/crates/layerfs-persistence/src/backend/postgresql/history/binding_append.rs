//! One overflow binding batch under the ordinary standalone write lane.
use super::super::session::attempt::nonce;
use super::fused::HINTS;
use crate::{
    backend::{
        error::BackendError,
        postgresql::session::{
            connection::Session,
            pipeline::{Refusal, Reply, Request},
            profile::barrier_leftover,
        },
    },
    storage::blocks::hints::Block,
};
use postgres::{error::SqlState, SimpleQueryMessage};
use std::sync::atomic::Ordering;

pub(crate) fn eligible(session: &Session) -> Result<bool, BackendError> {
    Ok(session.incarnation.load(Ordering::SeqCst) != 0 && !session.admission.held_by_caller()?)
}

pub(crate) fn run(session: &Session, blocks: Vec<Block>) -> Result<(), BackendError> {
    let generation = session.incarnation.load(Ordering::SeqCst);
    let schema = &session.settings.schema;
    let attempt = nonce();
    let begin = session.settings.write_begin(true);
    // As in Mode::Write, fence after acquiring the lane. A separate request
    // keeps a BEGIN/lane error distinguishable from a generation refusal.
    let fence = format!(
        "SELECT CASE WHEN g IS NULL THEN (32768 + (g IS NULL)::int)::smallint::int          ELSE 1/(g={generation})::int END FROM          (SELECT (SELECT catalog_incarnation FROM {schema}.history_meta WHERE id=1) AS g) m;          SELECT set_config('layerfs.attempt','{attempt}',true)"
    );
    let commit = format!(
        "SELECT current_setting('layerfs.attempt',true) WHERE          1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM '{attempt}')::int=1;          COMMIT"
    );
    let sql = HINTS.replace("{s}", schema);
    let groups: Vec<i64> = blocks.iter().map(|block| block.group).collect();
    let bytes = blocks
        .iter()
        .map(|block| block.body.len() as u64 + 8)
        .sum::<u64>();
    let bodies: Vec<Vec<u8>> = blocks.into_iter().map(|block| block.body).collect();
    let requests = [
        Request::Simple(&begin),
        Request::Simple(&fence),
        Request::Statement {
            sql: &sql,
            params: &[&groups, &bodies, &attempt],
            bytes: bytes + attempt.len() as u64,
        },
        Request::Simple(&commit),
    ];
    // A guard refusal skips the COMMIT in its simple message. End that
    // transaction and clear a lane barrier whose release was interrupted.
    let cleanup = format!("; ROLLBACK{}", barrier_leftover(schema));
    let outcome = session.pipeline_judged(&requests, &cleanup, |replies, trusted| {
        let result = decide(&replies, trusted, &attempt);
        (result, result == Err(BackendError::Unknown))
    })?;
    session.count_fused(outcome.is_ok());
    outcome
}

fn decide(
    replies: &[Result<Reply, Refusal>],
    trusted: bool,
    attempt: &str,
) -> Result<(), BackendError> {
    if !trusted {
        return Err(BackendError::Unknown);
    }
    for (index, reply) in replies.iter().enumerate() {
        if let Err(refusal) = reply {
            return Err(match &refusal.code {
                Some(code) if index == 1 && *code == SqlState::DIVISION_BY_ZERO => {
                    BackendError::Stale
                }
                Some(_) => refusal.error,
                None if !refusal.sent => refusal.error,
                None => BackendError::Unknown,
            });
        }
    }
    let [Ok(Reply::Messages(_)), Ok(Reply::Messages(fence)), Ok(Reply::Rows(rows)), Ok(Reply::Messages(commit))] =
        replies
    else {
        return Err(BackendError::Unknown);
    };
    use SimpleQueryMessage::{CommandComplete, Row, RowDescription};
    // Column metadata is included by the simple-query driver. The separate
    // successful guard disambiguates COMMIT from an aborted transaction.
    let valid_fence = matches!(
        fence.as_slice(),
        [RowDescription(_), Row(ok), CommandComplete(1), RowDescription(_), Row(token), CommandComplete(1)]
            if ok.try_get(0).ok().flatten() == Some("1")
            && token.try_get(0).ok().flatten() == Some(attempt)
    );
    let valid_commit = matches!(
        commit.as_slice(),
        [RowDescription(_), Row(token), CommandComplete(1), CommandComplete(0)]
            if token.try_get(0).ok().flatten() == Some(attempt)
    );
    if valid_fence && rows.is_empty() && valid_commit {
        Ok(())
    } else {
        Err(BackendError::Unknown)
    }
}

//! The host worker's PostgreSQL lane (#819 P-B): PostgreSQL reads and
//! single-change PostgreSQL writes, handed over by an ordinary thread once
//! their request is read and checked ([`HostServer::lane_request`]), run
//! on held sessions ([`layerfs_store::postgresql::pool`]).
//!
//! A lane thread starts the oldest queued call whose Store has a free slot
//! of the call's kind, so a call of a busy Store waits in the queue, on no
//! thread. A write whose Store or process lends its share of write slots
//! waits the same way, so reads keep at least half of each bound and the
//! lane threads a held Store writer can keep busy are bounded by that
//! share: neither the ordinary threads nor the read slots wait on a
//! PostgreSQL write. A streamed PostgreSQL write comes in on the write
//! endpoint and waits for its slot in the write lane (`write.rs`).
use super::{log, peer_gone, Activity, Endpoint, HostServer, Pending, Served, POSTGRES};
use layerfs_bridge::control::SdkValue;
use layerfs_store::postgresql::pool::{self, Kind, Reservation};
use std::os::unix::net::UnixStream;

/// Where a PostgreSQL call went.
pub(super) enum HandOver {
    Queued,
    /// Draining: answered by the ordinary thread.
    Back(UnixStream, SdkValue),
    /// The lane holds its `slots` calls: answered `Unavailable` at once,
    /// unrun.
    Full(UnixStream, SdkValue),
}

impl HostServer {
    /// Hands PostgreSQL `request` on `stream` to its lane, or hands it back
    /// at once: while draining, or when the lane holds its `slots` calls
    /// (`Full`); the ordinary thread never waits here.
    pub(super) fn hand_over(&self, stream: UnixStream, request: SdkValue) -> HandOver {
        let mut activity = self.lock();
        if activity.draining {
            return HandOver::Back(stream, request);
        }
        purge_gone(&mut activity);
        let queue = &mut activity.lanes[POSTGRES.index];
        if queue.outstanding >= POSTGRES.slots {
            return HandOver::Full(stream, request);
        }
        queue.outstanding += 1;
        queue.pending.push_back(Pending {
            stream,
            request: Some(request),
        });
        self.queued[POSTGRES.index].notify_all();
        HandOver::Queued
    }

    /// A PostgreSQL session slot came back: the lane and the write threads
    /// may start a call that waited for it.
    pub(super) fn slot_freed(&self) {
        self.queued[POSTGRES.index].notify_all();
        self.writable.notify_all();
    }

    /// The oldest queued PostgreSQL call whose Store has a free session
    /// slot of its kind, with that slot; draining, the oldest call, to be
    /// refused.
    fn pop_postgres(&self) -> Option<(Pending, Option<Reservation>)> {
        let mut activity = self.lock();
        loop {
            purge_gone(&mut activity);
            let draining = activity.draining;
            let pending = &mut activity.lanes[POSTGRES.index].pending;
            let mut chosen = None;
            for (at, job) in pending.iter().enumerate() {
                // every held call passed `runnable` at intake; this lookup
                // names its Store and kind
                let store = job
                    .request
                    .as_ref()
                    .and_then(|request| self.runnable(request));
                match store {
                    Some((store, kind)) if !draining => {
                        if let Some(slot) = pool::reserve_kind(&self.home, store, kind) {
                            chosen = Some((at, Some(slot)));
                            break;
                        }
                    }
                    // draining: refused before any session is taken
                    _ => {
                        chosen = Some((at, None));
                        break;
                    }
                }
            }
            if let Some((at, slot)) = chosen {
                return pending.remove(at).map(|job| (job, slot));
            }
            if draining {
                return None;
            }
            // every slot given back ends a call, whose `finish` (or write
            // thread) wakes us
            activity = (self.queued[POSTGRES.index].wait(activity))
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    /// PostgreSQL `request`, read by an ordinary thread, as its lane holds
    /// it (`Ok`), or `Err` with the request when the request alone refuses
    /// it, to be answered on that thread, holding no lane place: the lane
    /// would refuse it the same way, with no session taken. A held request
    /// is [`Self::runnable`] with a valid request ID, and keeps only the
    /// top-level fields a host request is read by (`op`, `generation`,
    /// `request_id`, `trace_step`, `args`; any other is ignored on every
    /// path), so it is small: text within its 64 KiB frame and at most 3
    /// containers and 45 fields and commas (a write's 7 envelope fields and
    /// 12 named ones, all scalar; a read's 6 and 8), which bounds the
    /// lane's decoded memory (workers-and-reach.md, per-lane table).
    pub(in crate::worker_server) fn lane_request(
        &self,
        request: SdkValue,
    ) -> Result<SdkValue, SdkValue> {
        let identified =
            (request.get_text("request_id")).is_some_and(|id| (1..=128).contains(&id.len()));
        if !identified || self.runnable(&request).is_none() {
            return Err(request);
        }
        let SdkValue::Map(fields) = request else {
            return Err(request);
        };
        let kept = fields.into_iter().filter(|(name, value)| {
            matches!(
                (name.as_str(), value),
                ("op" | "generation" | "request_id", SdkValue::Text(_))
                    | ("trace_step", SdkValue::Bool(_))
                    | ("args", _)
            )
        });
        Ok(SdkValue::Map(kept.collect()))
    }

    /// The Store and session kind of PostgreSQL call `request` when nothing
    /// in the request alone refuses it: this worker's generation, a read
    /// whose arguments pass the route's request checks or a single-change
    /// write whose header checks, and a Store name. The record is checked
    /// when it runs.
    fn runnable<'a>(&self, request: &'a SdkValue) -> Option<(&'a str, Kind)> {
        if request.get_text("generation") != Some(self.hello.generation.as_str()) {
            return None;
        }
        let args = request.get("args")?;
        let kind = match request.get_text("op")? {
            layerfs_store::host_postgresql::OP => {
                layerfs_store::host_postgresql::checks(args).then_some(Kind::Read)?
            }
            layerfs_store::host_write::OP => {
                let (.., body, _) = layerfs_store::host_write::header(args).ok()?;
                (body == "none").then_some(Kind::Write)?
            }
            _ => return None,
        };
        let store = args.get_text("store")?;
        layerfs_home::names::StoreName::parse(store).ok()?;
        Some((store, kind))
    }

    pub(super) fn work_postgres(&self) {
        while let Some((Pending { stream, request }, slot)) = self.pop_postgres() {
            let mut admitted = false;
            if let Some(slot) = slot {
                pool::hold(slot);
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::super::connection::serve_host_connection(
                    self,
                    &stream,
                    Endpoint::Ordinary,
                    request,
                    &mut admitted,
                )
            }));
            pool::release_held();
            if result.is_err() {
                let _ = layerfs_home::trace::steps_take();
                log("a host PostgreSQL call panicked; its answer was lost");
            }
            drop(stream);
            self.finish(POSTGRES, admitted, &result);
        }
    }

    /// Serves a PostgreSQL call an ordinary thread read: handed to its
    /// lane, or answered here when draining (refused, as every call
    /// admitted then) or when the lane is full (refused at once, unrun).
    pub(super) fn handed(
        &self,
        stream: UnixStream,
        request: SdkValue,
        serve: impl FnOnce(&UnixStream, SdkValue) -> std::thread::Result<Served>,
    ) -> (Option<UnixStream>, std::thread::Result<Served>) {
        match self.hand_over(stream, request) {
            HandOver::Back(open, request) => {
                let served = serve(&open, request);
                (Some(open), served)
            }
            HandOver::Full(open, request) => {
                let refusal = layerfs_home::outcome::refused(
                    "Unavailable",
                    format!(
                        "the host worker holds {} PostgreSQL calls not yet \
                         answered; this call was not run",
                        POSTGRES.slots
                    ),
                );
                let refusal = layerfs_bridge::control::SdkRefusal {
                    detail: SdkValue::map([
                        ("state", SdkValue::text("postgresql-reads-full")),
                        ("reads", SdkValue::Int(POSTGRES.slots as i64)),
                    ]),
                    ..refusal
                };
                let answered =
                    super::super::connection::refuse_host_request(self, &open, request, refusal);
                (Some(open), Ok(Served::Answered(answered)))
            }
            HandOver::Queued => (None, Ok(Served::HandedOver)),
        }
    }
}

/// Drops queued PostgreSQL calls whose caller has closed its end: none of
/// them has started, and none is run.
pub(super) fn purge_gone(activity: &mut Activity) {
    let queue = &mut activity.lanes[POSTGRES.index];
    let before = queue.pending.len();
    let mut gone = Vec::new();
    queue.pending.retain_mut(|job| {
        let open = !peer_gone(&job.stream);
        if !open {
            gone.push(job.request.take());
        }
        open
    });
    queue.outstanding -= before - queue.pending.len();
    for request in gone.into_iter().flatten() {
        layerfs_store::host_postgresql::wipe_request(request);
    }
}

/// The ping's `postgresql` field (§11): held sessions idle and lent (and
/// of those the write slots), and calls handed over and not yet answered,
/// and queued.
pub(super) fn ping(activity: &Activity) -> SdkValue {
    let counts = pool::counts();
    SdkValue::map([
        ("stores", SdkValue::Int(counts.stores as i64)),
        ("idle", SdkValue::Int(counts.idle as i64)),
        ("lent", SdkValue::Int(counts.lent as i64)),
        ("writes", SdkValue::Int(counts.writes as i64)),
        (
            "reads",
            SdkValue::Int(activity.lanes[POSTGRES.index].outstanding as i64),
        ),
        (
            "queued",
            SdkValue::Int(activity.lanes[POSTGRES.index].pending.len() as i64),
        ),
    ])
}

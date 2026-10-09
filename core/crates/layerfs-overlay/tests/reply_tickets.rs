//! Reply tickets held in the engine's memory, through the public API only:
//! one ticket per publishing job, none for a job that fails, attempts
//! recorded from other threads, the owner turn a watched last attempt owes
//! to a waiting capture or terminal cleanup, per-namespace isolation, paging
//! and counts. No private source or product hook; the threads spawned here
//! only make short calls that never wait for another thread.
mod payload_support;
use layerfs_overlay::*;
use payload_support::Temp;
use std::sync::Arc;

const PAGE: usize = 64;

fn file(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 4,
        mtime_nanoseconds: 5,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}
fn publish(db: &Overlay, route: Route, serial: u64) -> Publication {
    db.publish(route, &file(serial), None, None).unwrap()
}
/// One attempt's outcome in comparable form: `None` is `Stale`, the answer
/// for a ticket that is not held. Any other error fails the test.
fn attempt(tickets: &ReplyTickets, publication: Publication) -> Option<Settled> {
    match tickets.attempted(publication) {
        Ok(settled) => Some(settled),
        Err(OverlayError::Stale) => None,
        Err(other) => panic!("reply attempt: {other:?}"),
    }
}
fn pending(db: &Overlay, route: Route) -> Vec<Publication> {
    db.pending_publications(route, 0).unwrap()
}
/// Tickets of one namespace and of the whole engine, as `resources` counts.
fn counted(db: &Overlay, route: Route) -> (u64, u64) {
    (
        db.resources(Some(route)).unwrap().counts.reply_tickets,
        db.resources(None).unwrap().counts.reply_tickets,
    )
}
/// A publishing visit whose kernel custody names an inode that is not among
/// its finals: the job fails after it wrote its inode and advanced the
/// frontier, which is where its ticket is issued.
fn fails_after_frontier_advance(db: &Overlay, mount: NativeMount, request: u64, serial: u64) {
    let before = db.diagnostics();
    assert!(matches!(
        db.mutate_native_visit(mount, request, 1, None, |_, _| {
            Ok(Some((
                Changes {
                    inodes: vec![file(serial)],
                    ..Changes::default()
                },
                NativeEffect::Entry {
                    serial: serial + 1,
                    parent: 1,
                    directory: false,
                },
            )))
        }),
        Err(OverlayError::Invalid("native entry final"))
    ));
    let work = db.diagnostics().since(&before);
    // It did write, and all of it was rolled back.
    assert!(work.total().rows_changed > 0);
    assert_eq!(
        work.statements[StatementKind::Rollback as usize].executions,
        1
    );
    assert_eq!(
        work.statements[StatementKind::Commit as usize].executions,
        0
    );
}
fn reclaimed(db: &Overlay, route: Route) {
    for _ in 0..1000 {
        if db.reclaim_closed(0).unwrap().is_none() {
            break;
        }
    }
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Gone);
}

#[test]
fn every_publishing_job_issues_exactly_one_ticket_and_nothing_else_issues_any() {
    fn shared<T: Send + Sync>() {}
    shared::<ReplyTickets>();
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([1; 32], [2; 32]).unwrap();
    let mount = db.create_native_mount(route, 1).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    // Opening, mounting and acquiring publish nothing.
    assert!(pending(&db, route).is_empty());
    assert_eq!(counted(&db, route), (0, 0));

    // The first-path job.
    let sql = db.diagnostics();
    let first = publish(&db, route, 2);
    // The ticket is no row: the job changed its inode and the frontier.
    let work = db.diagnostics().since(&sql);
    assert_eq!(
        work.statements[StatementKind::Frontier as usize],
        StatementWork::default()
    );
    assert_eq!(first.route(), route);
    assert_eq!(first.revision(), db.state(route).unwrap().revision);
    assert_eq!(first.generation, db.state(route).unwrap().active);
    assert_eq!(pending(&db, route), vec![first]);
    assert_eq!(counted(&db, route), (1, 1));

    // A compound job of several finals is still one ticket.
    let compound = db
        .apply(
            source,
            &Changes {
                inodes: vec![file(3), file(4), file(5)],
                ..Changes::default()
            },
        )
        .unwrap();
    assert_eq!(compound.revision(), first.revision() + 1);
    assert_eq!(pending(&db, route), vec![first, compound]);
    assert_eq!(counted(&db, route), (2, 2));

    // A publishing visit, with its kernel custody, is one ticket.
    let visit = db
        .mutate_native_visit(mount, 7, 1, None, |_, _| {
            Ok(Some((
                Changes {
                    inodes: vec![file(6)],
                    ..Changes::default()
                },
                NativeEffect::None,
            )))
        })
        .unwrap()
        .expect("published");
    assert_eq!(visit.publication.revision(), compound.revision() + 1);
    assert_eq!(
        pending(&db, route),
        vec![first, compound, visit.publication]
    );
    assert_eq!(counted(&db, route), (3, 3));

    // A visit that decides without changes and reads issue nothing.
    assert_eq!(
        db.mutate_native_visit(mount, 8, 1, None, |_, _| Ok(None))
            .unwrap(),
        None
    );
    db.inode(route, 2).unwrap().unwrap();
    db.state(route).unwrap();
    assert_eq!(pending(&db, route).len(), 3);
    assert_eq!(counted(&db, route), (3, 3));

    // Each ticket is returned by exactly one attempt.
    let tickets = db.reply_tickets();
    assert_eq!(attempt(&tickets, compound), Some(Settled::Pending));
    assert_eq!(pending(&db, route), vec![first, visit.publication]);
    assert_eq!(attempt(&tickets, first), Some(Settled::Pending));
    assert_eq!(attempt(&tickets, visit.publication), Some(Settled::Last));
    assert!(pending(&db, route).is_empty());
    assert_eq!(counted(&db, route), (0, 0));
    // The handle is the engine's own record, not a copy of it.
    assert!(Arc::ptr_eq(&tickets, &db.reply_tickets()));
}

#[test]
fn a_job_that_fails_after_its_frontier_advance_leaves_no_ticket() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([3; 32], [4; 32]).unwrap();
    let mount = db.create_native_mount(route, 1).unwrap();
    let tickets = db.reply_tickets();

    // With nothing else pending: no ticket, no revision, and no waiter is
    // held back by a ticket that was never owed.
    let revision = db.state(route).unwrap().revision;
    fails_after_frontier_advance(&db, mount, 1, 10);
    assert_eq!(db.state(route).unwrap().revision, revision);
    assert_eq!(db.inode(route, 10).unwrap(), None);
    assert!(pending(&db, route).is_empty());
    assert_eq!(counted(&db, route), (0, 0));
    assert!(db.capture_ready(route).unwrap());

    // Beside an earlier ticket that a waiter watches: the failed job takes
    // back its own ticket only, and the waiter is still owed its turn.
    let earlier = publish(&db, route, 2);
    assert!(!db.capture_ready(route).unwrap());
    fails_after_frontier_advance(&db, mount, 2, 20);
    assert_eq!(db.state(route).unwrap().revision, earlier.revision());
    assert_eq!(pending(&db, route), vec![earlier]);
    assert_eq!(counted(&db, route), (1, 1));
    assert_eq!(attempt(&tickets, earlier), Some(Settled::Watched));
    assert!(db.capture_ready(route).unwrap());

    // The next publication takes the revision the failed job had advanced
    // to. Its ticket is held exactly once and nobody waits for it.
    let next = publish(&db, route, 3);
    assert_eq!(next.revision(), earlier.revision() + 1);
    assert_eq!(pending(&db, route), vec![next]);
    assert_eq!(attempt(&tickets, next), Some(Settled::Last));
    assert_eq!(attempt(&tickets, next), None);

    // A job refused before it writes anything issues nothing either.
    db.close(route).unwrap();
    assert!(matches!(
        db.publish(route, &file(4), None, None),
        Err(OverlayError::Closed)
    ));
    assert_eq!(counted(&db, route), (0, 0));
}

#[test]
fn attempts_from_other_threads_are_pending_then_last_and_a_repeat_is_stale() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([5; 32], [6; 32]).unwrap();
    let published: Vec<_> = (2..=4).map(|serial| publish(&db, route, serial)).collect();
    let tickets = db.reply_tickets();

    // The replying thread holds no connection: only the shared record.
    let replying = tickets.clone();
    let (first, second) = (published[0], published[1]);
    let outcomes = std::thread::spawn(move || {
        [first, second, first].map(|publication| attempt(&replying, publication))
    })
    .join()
    .unwrap();
    // The second attempt of one publication holds no ticket.
    assert_eq!(
        outcomes,
        [Some(Settled::Pending), Some(Settled::Pending), None]
    );
    // The owner sees what the other thread recorded, and the stale attempt
    // changed nothing.
    assert_eq!(pending(&db, route), vec![published[2]]);
    assert_eq!(counted(&db, route), (1, 1));
    assert!(matches!(
        db.capture(route),
        Err(OverlayError::ReplyAttemptsPending)
    ));
    // The owner-turn form refuses the same repeat and changes nothing.
    assert!(matches!(
        db.reply_attempted(first),
        Err(OverlayError::Stale)
    ));
    assert_eq!(pending(&db, route), vec![published[2]]);

    // No capture or cleanup observed the pending ticket: the last is Last.
    assert_eq!(attempt(&tickets, published[2]), Some(Settled::Last));
    assert_eq!(attempt(&tickets, published[2]), None);
    assert!(pending(&db, route).is_empty());
    assert_eq!(counted(&db, route), (0, 0));
    db.capture(route).unwrap();
}

#[test]
fn concurrent_attempts_return_every_ticket_once_and_one_of_them_settles_the_namespace() {
    const TICKETS: u64 = 256;
    const THREADS: usize = 4;
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([7; 32], [8; 32]).unwrap();
    let published: Vec<_> = (0..TICKETS)
        .map(|index| publish(&db, route, 2 + index))
        .collect();
    assert_eq!(counted(&db, route), (TICKETS, TICKETS));
    // A waiter observes them pending.
    assert!(!db.capture_ready(route).unwrap());
    let tickets = db.reply_tickets();

    // Every thread attempts every publication, each from its own offset.
    // None of them waits for another, so each one finishes on its own.
    let outcomes: Vec<Vec<Option<Settled>>> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..THREADS)
            .map(|thread| {
                let tickets = &tickets;
                let published = &published;
                scope.spawn(move || {
                    let start = thread * published.len() / THREADS;
                    (0..published.len())
                        .map(|step| attempt(tickets, published[(start + step) % published.len()]))
                        .collect()
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect()
    });
    let count = |wanted: Option<Settled>| {
        outcomes
            .iter()
            .flatten()
            .filter(|outcome| **outcome == wanted)
            .count() as u64
    };
    // One attempt per ticket succeeded; all the others held nothing.
    assert_eq!(count(Some(Settled::Pending)), TICKETS - 1);
    assert_eq!(count(Some(Settled::Watched)), 1);
    assert_eq!(count(Some(Settled::Last)), 0);
    assert_eq!(count(None), (THREADS as u64 - 1) * TICKETS);
    assert!(pending(&db, route).is_empty());
    assert_eq!(counted(&db, route), (0, 0));
    assert!(db.capture_ready(route).unwrap());
    db.reply_settled(route).unwrap();
    db.capture(route).unwrap();
}

#[test]
fn a_ticket_is_named_by_its_namespace_revision_and_generation() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([9; 32], [10; 32]).unwrap();
    let tickets = db.reply_tickets();
    let sealed = publish(&db, route, 2);
    assert_eq!(attempt(&tickets, sealed), Some(Settled::Last));
    let capture = db.capture(route).unwrap();
    assert_eq!(capture.generation, sealed.generation);
    assert_eq!(capture.revision, sealed.revision());
    // A publication into the next generation, at the next revision.
    let active = publish(&db, route, 3);
    assert_ne!(active.generation, sealed.generation);
    // The same revision under another generation is not that ticket.
    let mut other = active;
    other.generation = sealed.generation;
    assert_ne!(other, active);
    assert_eq!(attempt(&tickets, other), None);
    assert!(matches!(
        db.reply_attempted(other),
        Err(OverlayError::Stale)
    ));
    assert_eq!(pending(&db, route), vec![active]);
    assert_eq!(counted(&db, route), (1, 1));
    assert_eq!(attempt(&tickets, active), Some(Settled::Last));
}

#[test]
fn a_waiting_capture_is_owed_the_last_attempt_and_no_waiter_means_last() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([11; 32], [12; 32]).unwrap();
    let tickets = db.reply_tickets();

    // The capture attempt itself refuses and is no waiter: it marks nothing.
    let alone = publish(&db, route, 2);
    assert!(matches!(
        db.capture(route),
        Err(OverlayError::ReplyAttemptsPending)
    ));
    assert_eq!(attempt(&tickets, alone), Some(Settled::Last));

    // The readiness observation of a waiting capture marks the namespace.
    let first = publish(&db, route, 3);
    let second = publish(&db, route, 4);
    assert!(!db.capture_ready(route).unwrap());
    assert!(matches!(
        db.capture(route),
        Err(OverlayError::ReplyAttemptsPending)
    ));
    assert_eq!(attempt(&tickets, first), Some(Settled::Pending));
    // Still one pending: not ready, still refused, still marked.
    assert!(!db.capture_ready(route).unwrap());
    assert!(matches!(
        db.capture(route),
        Err(OverlayError::ReplyAttemptsPending)
    ));
    assert_eq!(attempt(&tickets, second), Some(Settled::Watched));
    // The owed owner turn has nothing to queue for a live Workspace and
    // writes nothing; the capture is ready.
    let sql = db.diagnostics();
    db.reply_settled(route).unwrap();
    let work = db.diagnostics().since(&sql).total();
    assert_eq!(work.rows_changed + work.direct_rows_changed, 0);
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Live);
    assert!(db.capture_ready(route).unwrap());

    // The mark was spent by the attempt it was owed to, and a ready
    // observation leaves none: the next publication has no waiter.
    let later = publish(&db, route, 5);
    assert_eq!(attempt(&tickets, later), Some(Settled::Last));
    // The capture seals every publication made before it.
    let capture = db.capture(route).unwrap();
    assert_eq!(capture.revision, later.revision());
    assert_eq!(capture.generation, later.generation);
}

#[test]
fn tickets_of_one_namespace_never_affect_another() {
    let temp = Temp::new();
    let db = temp.db();
    let a = db.open_workspace([13; 32], [14; 32]).unwrap();
    let b = db.open_workspace([15; 32], [14; 32]).unwrap();
    assert_ne!(a.namespace(), b.namespace());
    let tickets = db.reply_tickets();
    let in_a = publish(&db, a, 2);
    let in_b = publish(&db, b, 2);
    // Equal revisions and generations in two namespaces are two tickets.
    assert_eq!(in_a.revision(), in_b.revision());
    assert_eq!(in_a.generation, in_b.generation);
    assert_eq!(pending(&db, a), vec![in_a]);
    assert_eq!(pending(&db, b), vec![in_b]);
    assert_eq!(counted(&db, a), (1, 2));
    assert_eq!(counted(&db, b), (1, 2));

    // A waiter of A does not mark B, and B's attempt does not settle A.
    assert!(!db.capture_ready(a).unwrap());
    assert_eq!(attempt(&tickets, in_b), Some(Settled::Last));
    assert_eq!(attempt(&tickets, in_b), None);
    assert_eq!(pending(&db, a), vec![in_a]);
    assert!(pending(&db, b).is_empty());
    assert_eq!(counted(&db, a), (1, 1));
    assert_eq!(counted(&db, b), (0, 1));
    assert!(matches!(
        db.capture(a),
        Err(OverlayError::ReplyAttemptsPending)
    ));
    assert!(!db.capture_ready(a).unwrap());
    // B captures and closes while A's ticket is pending.
    assert!(db.capture_ready(b).unwrap());
    let sealed = db.capture(b).unwrap();
    db.close(b).unwrap();
    db.release_closed_capture(sealed).unwrap();
    assert_eq!(db.cleanup_state(b).unwrap(), CleanupState::Queued);
    assert_eq!(db.cleanup_state(a).unwrap(), CleanupState::Live);
    reclaimed(&db, b);
    // A's ticket and its waiter outlive B's whole namespace.
    assert_eq!(pending(&db, a), vec![in_a]);
    assert_eq!(counted(&db, a), (1, 1));
    assert_eq!(attempt(&tickets, in_a), Some(Settled::Watched));
    assert!(db.capture_ready(a).unwrap());
    db.capture(a).unwrap();
}

#[test]
fn a_closed_workspace_with_a_pending_ticket_is_queued_only_by_the_settling_owner_turn() {
    let temp = Temp::new();
    let db = temp.db();
    let tickets = db.reply_tickets();

    // The owner-turn form: the attempt and its turn are one call.
    let route = db.open_workspace([17; 32], [18; 32]).unwrap();
    let only = publish(&db, route, 2);
    db.close(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert!(db.reclaim_closed(0).unwrap().is_none());
    // A stale attempt neither returns the ticket nor queues the cleanup:
    // the same revision under a generation this namespace never had.
    let mut other = only;
    other.generation = {
        let next = db.open_workspace([19; 32], [18; 32]).unwrap();
        db.capture(next).unwrap();
        let later = publish(&db, next, 2);
        assert_eq!(attempt(&tickets, later), Some(Settled::Last));
        later.generation
    };
    assert_ne!(other, only);
    assert_eq!(attempt(&tickets, other), None);
    assert!(matches!(
        db.reply_attempted(other),
        Err(OverlayError::Stale)
    ));
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    db.reply_attempted(only).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Queued);
    reclaimed(&db, route);

    // The replying-thread form: the attempts are recorded without a turn,
    // and only the last one, which the close observed pending, owes it.
    let route = db.open_workspace([23; 32], [24; 32]).unwrap();
    let first = publish(&db, route, 2);
    let second = publish(&db, route, 3);
    db.close(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert_eq!(attempt(&tickets, first), Some(Settled::Pending));
    // An owner turn before the last attempt queues nothing.
    db.reply_settled(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert_eq!(attempt(&tickets, second), Some(Settled::Watched));
    // The attempt itself is no owner turn: nothing is queued by it.
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert!(db.reclaim_closed(0).unwrap().is_none());
    db.reply_settled(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Queued);
    // The turn is idempotent.
    db.reply_settled(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Queued);
    reclaimed(&db, route);
    assert!(db.pending_publications(route, 0).is_err());
    assert!(db.reply_settled(route).is_err());
}

#[test]
fn a_failed_attempt_and_release_gives_the_ticket_back_with_its_waiter() {
    let temp = Temp::new();
    let db = temp.db();
    let tickets = db.reply_tickets();

    // No waiter: the ticket comes back and is still nobody's to wait for.
    let route = db.open_workspace([25; 32], [26; 32]).unwrap();
    let released = db.acquire_base_source(route, 1).unwrap();
    db.release_base_source(released).unwrap();
    let ticket = publish(&db, route, 2);
    assert!(matches!(
        db.reply_attempted_and_release(ticket, released),
        Err(OverlayError::Stale)
    ));
    assert_eq!(pending(&db, route), vec![ticket]);
    assert_eq!(attempt(&tickets, ticket), Some(Settled::Last));

    // A waiting capture: the ticket comes back still watched.
    let ticket = publish(&db, route, 3);
    assert!(!db.capture_ready(route).unwrap());
    assert!(matches!(
        db.reply_attempted_and_release(ticket, released),
        Err(OverlayError::Stale)
    ));
    assert_eq!(pending(&db, route), vec![ticket]);
    assert_eq!(counted(&db, route), (1, 1));
    assert_eq!(attempt(&tickets, ticket), Some(Settled::Watched));
    assert!(db.capture_ready(route).unwrap());

    // A terminal cleanup waiting for the ticket: after the failed call the
    // Workspace is still held, and the ticket's one attempt still owes the
    // owner turn that queues it.
    let ticket = publish(&db, route, 4);
    db.close(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert!(matches!(
        db.reply_attempted_and_release(ticket, released),
        Err(OverlayError::Stale)
    ));
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert_eq!(pending(&db, route), vec![ticket]);
    assert_eq!(attempt(&tickets, ticket), Some(Settled::Watched));
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    db.reply_settled(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Queued);

    // The successful call is the attempt and the release in one turn: a
    // closed Workspace held by both is queued by it.
    let route = db.open_workspace([27; 32], [26; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let ticket = publish(&db, route, 2);
    db.close(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    db.reply_attempted_and_release(ticket, source).unwrap();
    assert!(db.pending_publications(route, 0).unwrap().is_empty());
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Queued);
    assert_eq!(attempt(&tickets, ticket), None);
}

#[test]
fn pending_publications_page_in_ascending_revision_order_64_per_page() {
    const PUBLISHED: u64 = 150;
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([29; 32], [30; 32]).unwrap();
    let other = db.open_workspace([31; 32], [30; 32]).unwrap();
    let mut published = Vec::new();
    for index in 0..PUBLISHED {
        published.push(publish(&db, route, 2 + index));
        if index % 50 == 0 {
            publish(&db, other, 2 + index);
        }
    }
    assert_eq!(counted(&db, route), (PUBLISHED, PUBLISHED + 3));
    assert_eq!(counted(&db, other), (3, PUBLISHED + 3));
    let revisions = |page: &[Publication]| page.iter().map(|p| p.revision()).collect::<Vec<_>>();
    let pages = |db: &Overlay| {
        let mut pages = Vec::new();
        let mut after = 0;
        for _ in 0..=PUBLISHED {
            let sql = db.diagnostics();
            let page = db.pending_publications(route, after).unwrap();
            // The page is read from memory after the route's point check.
            let work = db.diagnostics().since(&sql);
            assert_eq!(
                work.statements[StatementKind::Frontier as usize],
                StatementWork::default()
            );
            assert!(page.len() <= PAGE);
            let Some(last) = page.last() else {
                return pages;
            };
            assert!(page.iter().all(|p| p.route() == route));
            assert!(page
                .windows(2)
                .all(|pair| pair[0].revision() < pair[1].revision()));
            assert!(page[0].revision() > after as i64);
            after = last.revision() as u64;
            pages.push(page);
        }
        panic!("paging did not end");
    };
    let all = pages(&db);
    assert_eq!(
        all.iter().map(Vec::len).collect::<Vec<_>>(),
        [PAGE, PAGE, PUBLISHED as usize - 2 * PAGE]
    );
    assert_eq!(all.concat(), published);
    assert_eq!(revisions(&all[0]), (1..=64).collect::<Vec<_>>());
    assert_eq!(revisions(&all[1]), (65..=128).collect::<Vec<_>>());
    // A cursor inside a page, on its last row and past the end.
    assert_eq!(
        revisions(&db.pending_publications(route, 100).unwrap()),
        (101..=150).collect::<Vec<_>>()
    );
    assert_eq!(
        revisions(&db.pending_publications(route, 149).unwrap()),
        [150]
    );
    assert!(db
        .pending_publications(route, PUBLISHED)
        .unwrap()
        .is_empty());
    assert!(db.pending_publications(route, u64::MAX).is_err());

    // Attempted tickets leave the pages; the rest keep their order.
    let tickets = db.reply_tickets();
    let mut left = published.clone();
    for revision in [1_i64, 64, 65, 100, 150] {
        let at = left.iter().position(|p| p.revision() == revision).unwrap();
        assert_eq!(attempt(&tickets, left.remove(at)), Some(Settled::Pending));
    }
    let all = pages(&db);
    assert_eq!(
        all.iter().map(Vec::len).collect::<Vec<_>>(),
        [PAGE, PAGE, PUBLISHED as usize - 5 - 2 * PAGE]
    );
    assert_eq!(all.concat(), left);
    assert_eq!(all[0][0].revision(), 2);
    assert_eq!(all[0][PAGE - 1].revision(), 67);
    assert_eq!(counted(&db, route), (PUBLISHED - 5, PUBLISHED - 2));
    // The other namespace's page is its own three tickets.
    assert_eq!(revisions(&pending(&db, other)), [1, 2, 3]);
}

#[test]
fn the_owner_turn_form_refuses_a_publication_of_another_engine() {
    let (a, b) = (Temp::new(), Temp::new());
    let (first, second) = (a.db(), b.db());
    let route_a = first.open_workspace([41; 32], [42; 32]).unwrap();
    let route_b = second.open_workspace([41; 32], [42; 32]).unwrap();
    // Equal local keys in two engines: namespace, revision and generation.
    assert_eq!(route_a.namespace(), route_b.namespace());
    assert_ne!(route_a, route_b);
    let foreign = publish(&first, route_a, 2);
    let own = publish(&second, route_b, 2);
    assert_eq!(
        (foreign.revision(), foreign.generation),
        (own.revision(), own.generation)
    );
    assert_ne!(foreign, own);
    // The owner-turn forms check the route against their engine before
    // they touch the record, and change nothing.
    assert!(matches!(
        second.reply_attempted(foreign),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        second.reply_settled(route_a),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        second.pending_publications(route_a, 0),
        Err(OverlayError::Stale)
    ));
    assert_eq!(pending(&second, route_b), vec![own]);
    assert_eq!(pending(&first, route_a), vec![foreign]);
    assert_eq!(counted(&second, route_b), (1, 1));
    // Each engine's record is its own.
    assert!(!Arc::ptr_eq(
        &first.reply_tickets(),
        &second.reply_tickets()
    ));
    // The record itself refuses another engine's publication, whatever
    // thread presents it, and keeps its own ticket of the same local key.
    assert_eq!(attempt(&second.reply_tickets(), foreign), None);
    assert_eq!(pending(&second, route_b), vec![own]);
    assert_eq!(attempt(&second.reply_tickets(), own), Some(Settled::Last));
    assert_eq!(pending(&first, route_a), vec![foreign]);
}

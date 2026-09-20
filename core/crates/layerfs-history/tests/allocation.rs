//! Scope-wide reservation arithmetic and the checked terminal endpoint.
mod support;

use layerfs_history::*;
use support::*;

#[test]
fn reservations_are_monotone_half_open_and_per_scope() {
    let temp = Temp::new("allocation");
    let catalog = create(&temp.join("catalog.sqlite"));
    let first = root(0xa0);
    let second = root(0xa1);
    let mut expected = 1u64;
    for count in [1u64, 2, 5, 1] {
        let reservation = catalog
            .reserve_inodes(&ReserveRequest {
                scope: first,
                count,
            })
            .unwrap();
        assert_eq!(reservation.start, expected);
        assert_eq!(reservation.count, count);
        assert_eq!(reservation.end().unwrap(), expected + count);
        expected += count;
    }
    // A different scope starts from its own mark and never shares a range.
    let other = catalog
        .reserve_inodes(&ReserveRequest {
            scope: second,
            count: 3,
        })
        .unwrap();
    assert_eq!(other.start, 1);
    assert_eq!(other.scope, second);
    let continued = catalog
        .reserve_inodes(&ReserveRequest {
            scope: first,
            count: 1,
        })
        .unwrap();
    assert_eq!(continued.start, expected);
}

#[test]
fn a_consumed_reservation_survives_a_later_failure() {
    let temp = Temp::new("allocation-failure");
    let catalog = create(&temp.join("catalog.sqlite"));
    let scope = root(0xb0);
    let reserved = catalog
        .reserve_inodes(&ReserveRequest { scope, count: 4 })
        .unwrap();
    assert_eq!(reserved.start, 1);
    // A successful creation and a failing one both leave the mark advanced: the
    // range was consumed when it was handed out, not when it was used.
    assert!(catalog
        .initialize_layerstack(&StackInitialization {
            stack: stack(0xb1),
            name: name("main"),
            scope,
            profile: root(0xb2),
            genesis_root: root(0xb3),
        })
        .is_ok());
    assert!(catalog
        .initialize_layerstack(&StackInitialization {
            stack: stack(0xb1),
            name: name("other"),
            scope,
            profile: root(0xb2),
            genesis_root: root(0xb5),
        })
        .is_err());
    let next = catalog
        .reserve_inodes(&ReserveRequest { scope, count: 1 })
        .unwrap();
    assert_eq!(next.start, 5);
}

#[test]
fn counts_are_checked_and_the_terminal_endpoint_is_refused() {
    let temp = Temp::new("allocation-bounds");
    let catalog = create(&temp.join("catalog.sqlite"));
    let scope = root(0xc0);
    assert_eq!(
        catalog
            .reserve_inodes(&ReserveRequest { scope, count: 0 })
            .unwrap_err(),
        HistoryError::InvalidInput("inode reservation")
    );
    assert_eq!(
        catalog
            .reserve_inodes(&ReserveRequest {
                scope,
                count: MAXIMUM_INODE_RESERVATION + 1,
            })
            .unwrap_err(),
        HistoryError::InvalidInput("inode reservation")
    );
    let accepted = catalog
        .reserve_inodes(&ReserveRequest {
            scope,
            count: MAXIMUM_INODE_RESERVATION,
        })
        .unwrap();
    assert_eq!(accepted.count, MAXIMUM_INODE_RESERVATION);
    // The terminal endpoint is a checked refusal, not an overflow: a range that
    // would pass `i64::MAX` cannot be represented and cannot be reported.
    let terminal = Reservation {
        scope,
        start: i64::MAX as u64,
        count: 1,
    };
    assert_eq!(
        terminal.end().unwrap_err(),
        HistoryError::Capacity("inode reservation")
    );
    let exact = Reservation {
        scope,
        start: i64::MAX as u64,
        count: 0,
    };
    assert_eq!(exact.end().unwrap(), i64::MAX as u64);
}

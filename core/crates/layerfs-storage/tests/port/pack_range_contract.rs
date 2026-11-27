//! Selected bytes must come from a complete authenticated source scan.
use layerfs_storage::{
    location::{PackDomain, PackInfo},
    port::*,
};

struct Plan(PackReadChoice);
impl PackReadPlan for Plan {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        Ok(self.0.clone())
    }
}
fn descriptor(body: &[u8]) -> PackInfo {
    PackInfo {
        pack_id: 1,
        domain: PackDomain::Payload,
        length: body.len(),
        key: ObjectKey::for_bytes(body),
    }
}
#[test]
fn ranges_crossing_prefix_and_scan_chunks_are_exact_and_all_bytes_are_paid_once() {
    let body: Vec<_> = (0..200_000).map(|n| (n * 37) as u8).collect();
    let ranges = vec![
        PackRange {
            offset: PACK_READ_PREFIX_BYTES - 13,
            length: 101,
        },
        PackRange {
            offset: PACK_READ_PREFIX_BYTES + PACK_SCAN_BYTES - 17,
            length: 211,
        },
        PackRange {
            offset: body.len() - 29,
            length: 29,
        },
    ];
    let mut calls = Vec::new();
    let read = PersistedPackRead::acquire(
        descriptor(&body),
        &mut Plan(PackReadChoice::Ranges(ranges.clone())),
        |offset, out| {
            calls.push((offset, out.len()));
            out.copy_from_slice(&body[offset..offset + out.len()]);
            Ok(())
        },
    )
    .unwrap();
    let PersistedPackRead::Ranges(read) = read else {
        panic!("selected strategy changed")
    };
    assert_eq!(read.info(), descriptor(&body));
    assert_eq!(read.prefix(), &body[..PACK_READ_PREFIX_BYTES]);
    for ((range, actual), expected) in read.ranges().iter().zip(ranges) {
        assert_eq!(*range, expected);
        assert_eq!(actual, &body[range.offset..range.offset + range.length]);
    }
    assert_eq!(calls.iter().map(|(_, n)| n).sum::<usize>(), body.len());
    for pair in calls.windows(2) {
        assert_eq!(pair[0].0 + pair[0].1, pair[1].0);
    }
}
#[test]
fn corruption_outside_selected_ranges_is_still_refused() {
    let mut body = vec![7; 100_000];
    let info = descriptor(&body);
    body[90_000] ^= 1;
    let result = PersistedPackRead::acquire(
        info,
        &mut Plan(PackReadChoice::Ranges(vec![PackRange {
            offset: 5000,
            length: 64,
        }])),
        |offset, out| {
            out.copy_from_slice(&body[offset..offset + out.len()]);
            Ok(())
        },
    );
    assert_eq!(result, Err(PersistenceError::Malformed));
}
#[test]
fn invalid_ranges_fail_without_body_scan_or_whole_strategy_fallback() {
    let body = vec![1; 10_000];
    for ranges in [
        vec![],
        vec![PackRange {
            offset: 0,
            length: 0,
        }],
        vec![PackRange {
            offset: 9999,
            length: 2,
        }],
        vec![PackRange {
            offset: usize::MAX,
            length: 2,
        }],
        vec![
            PackRange {
                offset: 20,
                length: 20,
            },
            PackRange {
                offset: 30,
                length: 10,
            },
        ],
    ] {
        let mut calls = 0;
        let result = PersistedPackRead::acquire(
            descriptor(&body),
            &mut Plan(PackReadChoice::Ranges(ranges)),
            |offset, out| {
                calls += 1;
                out.copy_from_slice(&body[offset..offset + out.len()]);
                Ok(())
            },
        );
        assert_eq!(result, Err(PersistenceError::Malformed));
        assert_eq!(calls, 1);
    }
}
#[test]
fn whole_selection_keeps_the_existing_carrier_and_io_failure_is_terminal() {
    let body = vec![9; 100_000];
    let read = PersistedPackRead::acquire(
        descriptor(&body),
        &mut Plan(PackReadChoice::Whole),
        |offset, out| {
            out.copy_from_slice(&body[offset..offset + out.len()]);
            Ok(())
        },
    )
    .unwrap();
    let PersistedPackRead::Whole(read) = read else {
        panic!("whole strategy changed")
    };
    assert_eq!(read.body(), body);
    let mut calls = 0;
    assert_eq!(
        PersistedPackRead::acquire(
            descriptor(&body),
            &mut Plan(PackReadChoice::Ranges(vec![PackRange {
                offset: 5000,
                length: 64
            }])),
            |_, _| {
                calls += 1;
                Err(PersistenceError::Uncertain)
            }
        ),
        Err(PersistenceError::Uncertain)
    );
    assert_eq!(calls, 1);
}

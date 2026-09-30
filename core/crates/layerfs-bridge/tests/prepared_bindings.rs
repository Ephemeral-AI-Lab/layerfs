//! Independent v1 bytes and scalar callback/budget/EOF facts, no speed measurement.
use layerfs_bridge::contract::*;
use std::io::{Cursor, Read};

#[derive(Default)]
struct Observed {
    begun: u64,
    bindings: Vec<(Vec<u8>, Option<u64>)>,
    completed: Vec<PreparedDirectoryCompletion>,
    identities: u64,
}
impl PreparedBindingSink for Observed {
    fn begin_directory(&mut self, parent: u64, _: u32) -> Result<(), Failure> {
        assert_eq!(parent, 1);
        self.begun += 1;
        Ok(())
    }
    fn binding(&mut self, name: &[u8], child: Option<u64>) -> Result<(), Failure> {
        self.bindings.push((name.to_vec(), child));
        Ok(())
    }
    fn end_directory(&mut self, completion: PreparedDirectoryCompletion) -> Result<(), Failure> {
        self.completed.push(completion);
        Ok(())
    }
    fn identity(&mut self, _: PreparedIdentity) -> Result<(), Failure> {
        self.identities += 1;
        Ok(())
    }
}

struct Counted {
    source: Cursor<Vec<u8>>,
    bytes: usize,
    maximum: usize,
}
impl Read for Counted {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.maximum = self.maximum.max(buffer.len());
        let count = self.source.read(buffer)?;
        self.bytes += count;
        Ok(count)
    }
}
fn body(bindings: &[(Vec<u8>, Option<u64>)]) -> (PreparedTotals, Vec<u8>) {
    let name_bytes = bindings
        .iter()
        .map(|(name, _)| 10 + name.len() as u64)
        .sum();
    let totals = PreparedTotals {
        directories: 1,
        names: bindings.len() as u64,
        name_bytes,
        ..PreparedTotals::default()
    };
    // Transcribed wire: version1, parent1, count32, full u16-length/name/u64 child.
    let mut bytes = vec![1];
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&(bindings.len() as u32).to_be_bytes());
    for (name, child) in bindings {
        bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(&child.unwrap_or(0).to_be_bytes());
    }
    (totals, bytes)
}

#[test]
fn full_names_and_tombstones_stream_with_exact_completion_and_bounded_reads() {
    let expected = (0..129)
        .map(|index| {
            let mut name = format!("name{index:03}").into_bytes();
            name.resize(255, b'x');
            (name, (index % 3 != 0).then_some(index + 2))
        })
        .collect::<Vec<_>>();
    let (totals, bytes) = body(&expected);
    let length = bytes.len();
    let mut input = Counted {
        source: Cursor::new(bytes),
        bytes: 0,
        maximum: 0,
    };
    let mut sink = Observed::default();
    read_prepared_bindings(&totals, 1, &mut input, &mut sink).unwrap();
    assert_eq!(sink.bindings, expected);
    assert_eq!(sink.begun, 1);
    assert_eq!(
        sink.completed,
        [PreparedDirectoryCompletion {
            parent: 1,
            bindings: 129,
            wire_name_bytes: 129 * 265
        }]
    );
    assert_eq!(sink.identities, 0);
    assert_eq!(input.bytes, length);
    assert_eq!(input.maximum, 255);
}

#[test]
fn oversized_count_cannot_borrow_identity_bytes_or_begin_a_sink_row() {
    let (mut totals, mut bytes) = body(&[(b"a".to_vec(), Some(2))]);
    totals.identities = 2;
    bytes[9..13].copy_from_slice(&2u32.to_be_bytes());
    bytes.resize(bytes.len() + 146, 0);
    let mut input = Counted {
        source: Cursor::new(bytes),
        bytes: 0,
        maximum: 0,
    };
    let mut sink = Observed::default();
    assert_eq!(
        read_prepared_bindings(&totals, 1, &mut input, &mut sink)
            .unwrap_err()
            .code,
        Code::Capacity
    );
    assert_eq!(input.bytes, 13);
    assert_eq!(sink.begun, 0);
    assert!(sink.bindings.is_empty() && sink.completed.is_empty());
    assert_eq!(sink.identities, 0);
}

#[test]
fn name_budget_refuses_before_copying_or_emitting_oversized_name() {
    let (totals, mut bytes) = body(&[(b"a".to_vec(), Some(2))]);
    bytes[13..15].copy_from_slice(&255u16.to_be_bytes());
    let mut input = Counted {
        source: Cursor::new(bytes),
        bytes: 0,
        maximum: 0,
    };
    let mut sink = Observed::default();
    assert_eq!(
        read_prepared_bindings(&totals, 1, &mut input, &mut sink)
            .unwrap_err()
            .code,
        Code::Capacity
    );
    assert_eq!(input.bytes, 15);
    assert_eq!(input.maximum, 8);
    assert_eq!(sink.begun, 1);
    assert!(sink.bindings.is_empty() && sink.completed.is_empty());
}

#[test]
fn order_error_at_record129_never_emits_that_record_or_completion() {
    let mut names = (0..129)
        .map(|index| (format!("name{index:03}").into_bytes(), Some(index + 2)))
        .collect::<Vec<_>>();
    names[128] = names[127].clone();
    let (totals, bytes) = body(&names);
    let mut sink = Observed::default();
    assert_eq!(
        read_prepared_bindings(&totals, 1, &mut &bytes[..], &mut sink)
            .unwrap_err()
            .code,
        Code::InvalidInput
    );
    assert_eq!(sink.bindings, names[..128]);
    assert!(sink.completed.is_empty());
}

#[test]
fn row_completion_does_not_credit_missing_or_surplus_global_eof() {
    let (totals, bytes) = body(&[(b"a".to_vec(), None)]);
    for surplus in [false, true] {
        let mut changed = bytes.clone();
        if surplus {
            changed.push(99);
        } else {
            changed.pop();
        }
        let mut sink = Observed::default();
        assert_eq!(
            read_prepared_bindings(&totals, 1, &mut &changed[..], &mut sink)
                .unwrap_err()
                .code,
            Code::InvalidInput
        );
        assert_eq!(sink.completed.len(), usize::from(surplus));
        assert_eq!(sink.bindings.len(), usize::from(surplus));
    }
}

#[test]
fn actual_fresh_roles_must_match_the_declared_fresh_total() {
    let totals = PreparedTotals {
        identities: 1,
        ..PreparedTotals::default()
    };
    let mut bytes = vec![1, 3];
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&[11; 32]);
    bytes.extend_from_slice(&[13; 32]);
    let mut sink = Observed::default();
    assert_eq!(
        read_prepared_bindings(&totals, 1, &mut &bytes[..], &mut sink)
            .unwrap_err()
            .code,
        Code::InvalidInput
    );
    assert_eq!(sink.identities, 0);
    assert!(sink.completed.is_empty());
}

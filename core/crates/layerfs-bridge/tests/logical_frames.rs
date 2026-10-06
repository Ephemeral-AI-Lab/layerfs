//! Public checked fragments, multiplexed roots and aggregate receive ownership.
use layerfs_bridge::{
    codec::{self, Reassembly, ReassemblyConfig, ReceiveBudget},
    contract::*,
};
fn envelope(message: u64, class: MessageClass, total: u64) -> Envelope {
    Envelope {
        message,
        correlation: 100 + message,
        kind: MessageKind::Request,
        class,
        total_bytes: total,
    }
}
fn config() -> ReassemblyConfig {
    ReassemblyConfig {
        kind: MessageKind::Request,
        messages: 8,
        demand_messages: 1,
        control_messages: 1,
        message_bytes: 1 << 20,
        bytes: 2 << 20,
        demand_reserve: 1 << 19,
        control_reserve: 4096,
    }
}
#[test]
fn header_is_exact_borrowed_and_refuses_invalid_wire_without_body_allocation() {
    let body = b"canonical bytes";
    let frame = Fragment {
        envelope: envelope(1, MessageClass::Save, body.len() as u64),
        offset: 0,
        bytes: body,
    };
    let mut record = vec![0; MAX_RECORD_BYTES];
    let n = codec::encode(frame, &mut record).unwrap();
    let decoded = codec::decode(&record[..n]).unwrap();
    assert_eq!(decoded.envelope, frame.envelope);
    assert_eq!(decoded.bytes, body);
    assert!(decoded.is_end());
    assert_eq!(decoded.bytes.as_ptr(), record[HEADER_BYTES..].as_ptr());
    for index in [0, 4, 5, 6, 7] {
        let mut wrong = record[..n].to_vec();
        wrong[index] = 255;
        assert!(codec::decode(&wrong).is_err());
    }
    assert!(codec::decode(&record[..HEADER_BYTES - 1]).is_err());
    assert!(codec::decode(&vec![0; MAX_RECORD_BYTES + 1]).is_err());
    assert!(matches!(
        codec::encode(
            Fragment {
                envelope: frame.envelope,
                offset: u64::MAX,
                bytes: body
            },
            &mut record
        ),
        Err(FrameError::Invalid(_))
    ));
    assert!(!Fragment {
        envelope: frame.envelope,
        offset: u64::MAX,
        bytes: body
    }
    .is_end());
    let empty = Fragment {
        envelope: envelope(2, MessageClass::Control, 0),
        offset: 0,
        bytes: &[],
    };
    let n = codec::encode(empty, &mut record).unwrap();
    assert_eq!(n, HEADER_BYTES);
    assert!(codec::decode(&record[..n]).unwrap().is_end());
}
#[test]
fn interleaved_messages_finish_out_of_order_with_exact_correlation_and_credit() {
    let mut input = Reassembly::new(config()).unwrap();
    let a = envelope(1, MessageClass::Save, 7);
    let b = envelope(2, MessageClass::Demand, 4);
    assert!(input
        .push(Fragment {
            envelope: a,
            offset: 0,
            bytes: b"abc"
        })
        .unwrap()
        .is_none());
    let small = input
        .push(Fragment {
            envelope: b,
            offset: 0,
            bytes: b"root",
        })
        .unwrap()
        .unwrap();
    assert_eq!(small.envelope().correlation, 102);
    assert_eq!(small.bytes(), b"root");
    let large = input
        .push(Fragment {
            envelope: a,
            offset: 3,
            bytes: b"defg",
        })
        .unwrap()
        .unwrap();
    assert_eq!(large.bytes(), b"abcdefg");
    assert_eq!(large.envelope().correlation, 101);
    let work = input.work().unwrap();
    assert_eq!(work.live_messages, 2);
    assert_eq!(work.copied_bytes, 11);
    assert_eq!(work.fragments, 3);
    assert_eq!(work.allocation_attempts, 2);
    assert_eq!(work.requested_body_bytes, 11);
    assert!(matches!(
        input.push(Fragment {
            envelope: a,
            offset: 0,
            bytes: b"a"
        }),
        Err(FrameError::Invalid(_))
    ));
    drop((small, large));
    assert_eq!(input.work().unwrap().live_messages, 0);
    assert_eq!(input.work().unwrap().credited_bytes, 0);
}
#[test]
fn malformed_continuation_retains_original_partial_bytes_until_explicit_fence() {
    let mut input = Reassembly::new(config()).unwrap();
    let env = envelope(1, MessageClass::Save, 8);
    input
        .push(Fragment {
            envelope: env,
            offset: 0,
            bytes: b"orig",
        })
        .unwrap();
    for frame in [
        Fragment {
            envelope: env,
            offset: 3,
            bytes: b"x",
        },
        Fragment {
            envelope: env,
            offset: 5,
            bytes: b"x",
        },
        Fragment {
            envelope: Envelope {
                correlation: 900,
                ..env
            },
            offset: 4,
            bytes: b"x",
        },
    ] {
        assert!(input.push(frame).is_err());
    }
    let partial = input.drain_partial();
    assert_eq!(partial.len(), 1);
    assert!(!partial[0].complete());
    assert_eq!(partial[0].bytes(), b"orig");
    assert_eq!(partial[0].envelope(), env);
    assert_eq!(input.work().unwrap().live_messages, 1);
    drop(partial);
    assert_eq!(input.work().unwrap().live_messages, 0);
}
#[test]
fn aggregate_budget_spans_receivers_and_keeps_control_demand_reserves() {
    let mut cfg = config();
    cfg.messages = 3;
    cfg.bytes = 2000;
    cfg.message_bytes = 1000;
    cfg.demand_reserve = 500;
    cfg.control_reserve = 500;
    let budget = ReceiveBudget::new(cfg).unwrap();
    let mut a = Reassembly::with_budget(budget.clone()).unwrap();
    let mut b = Reassembly::with_budget(budget.clone()).unwrap();
    let saved = a
        .push(Fragment {
            envelope: envelope(1, MessageClass::Save, 700),
            offset: 0,
            bytes: &vec![7; 700],
        })
        .unwrap()
        .unwrap();
    assert!(matches!(
        b.push(Fragment {
            envelope: envelope(1, MessageClass::Save, 300),
            offset: 0,
            bytes: &vec![3; 300]
        }),
        Err(FrameError::AdmissionUnavailable)
    ));
    let demand = b
        .push(Fragment {
            envelope: envelope(2, MessageClass::Demand, 400),
            offset: 0,
            bytes: &vec![4; 400],
        })
        .unwrap()
        .unwrap();
    let control = b
        .push(Fragment {
            envelope: envelope(3, MessageClass::Control, 100),
            offset: 0,
            bytes: &[5; 100],
        })
        .unwrap()
        .unwrap();
    assert_eq!(budget.work().unwrap().live_messages, 3);
    assert!(matches!(
        a.push(Fragment {
            envelope: envelope(4, MessageClass::Control, 1),
            offset: 0,
            bytes: b"x"
        }),
        Err(FrameError::AdmissionUnavailable)
    ));
    drop((saved, demand, control));
    assert_eq!(budget.work().unwrap().credited_bytes, 0);
    // Old direction-local IDs stay burned while another receiver's namespace is independent.
    assert!(a
        .push(Fragment {
            envelope: envelope(1, MessageClass::Control, 1),
            offset: 0,
            bytes: b"x"
        })
        .is_err());
}
#[test]
fn completed_body_credit_survives_reassembly_owner_drop() {
    let budget = ReceiveBudget::new(config()).unwrap();
    let mut input = Reassembly::with_budget(budget.clone()).unwrap();
    let message = input
        .push(Fragment {
            envelope: envelope(1, MessageClass::Control, 5),
            offset: 0,
            bytes: b"saved",
        })
        .unwrap()
        .unwrap();
    drop(input);
    assert_eq!(message.bytes(), b"saved");
    assert_eq!(budget.work().unwrap().live_messages, 1);
    drop(message);
    assert_eq!(budget.work().unwrap().live_messages, 0);
}

#[test]
fn incomplete_saves_cannot_consume_demand_and_control_message_slots() {
    let mut c = config();
    c.messages = 3;
    c.demand_messages = 1;
    c.control_messages = 1;
    let budget = ReceiveBudget::new(c).unwrap();
    let mut a = Reassembly::with_budget(budget.clone()).unwrap();
    let mut b = Reassembly::with_budget(budget.clone()).unwrap();
    a.push(Fragment {
        envelope: envelope(1, MessageClass::Save, 2),
        offset: 0,
        bytes: b"a",
    })
    .unwrap();
    assert!(matches!(
        b.push(Fragment {
            envelope: envelope(1, MessageClass::Save, 2),
            offset: 0,
            bytes: b"b"
        }),
        Err(FrameError::AdmissionUnavailable)
    ));
    let demand = b
        .push(Fragment {
            envelope: envelope(2, MessageClass::Demand, 1),
            offset: 0,
            bytes: b"d",
        })
        .unwrap()
        .unwrap();
    let control = b
        .push(Fragment {
            envelope: envelope(3, MessageClass::Control, 1),
            offset: 0,
            bytes: b"c",
        })
        .unwrap()
        .unwrap();
    let (_, bytes, lease) = control.into_parts();
    assert_eq!(bytes, b"c");
    drop((demand, a, b));
    assert_eq!(budget.work().unwrap().live_messages, 1);
    drop(lease);
    assert_eq!(budget.work().unwrap().live_messages, 0);
}

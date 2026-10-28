//! Additive controls preserve old bytes and reject recursive observation input.
use layerfs_bridge::control::{
    Answer, Call, CleanupObservation, CommitControl, HelloRequest, Reply, Request, UnmountControl,
    WorkspaceToken,
};
use layerfs_history::WorkspaceId;

fn token() -> WorkspaceToken {
    WorkspaceToken {
        workspace: WorkspaceId::from_authority([3; 32]).unwrap(),
        namespace: 7,
    }
}
fn prefix(magic: &[u8], tag: u8) -> Vec<u8> {
    let mut bytes = magic.to_vec();
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.push(tag);
    bytes
}
#[test]
fn actual_inline_initial_record_layout_has_no_large_variant_gap() {
    use layerfs_bridge::{initial_record::InitialRecord, provision::StoreManifest};
    let request = std::mem::size_of::<Request>();
    let call = std::mem::size_of::<Call>();
    let install = std::mem::size_of::<StoreManifest>();
    let initial = std::mem::size_of::<InitialRecord>();
    let delta = install.abs_diff(call);
    println!("INITIAL_RECORD_LAYOUT request={request} call={call} store_manifest={install} initial_record={initial} variant_delta={delta}");
    assert!(
        request > 0 && call > 0 && initial > 0,
        "compiler layout must be available despite recursive boxed Request"
    );
    assert!(
        delta < 200,
        "actual inline variant size gap reaches the Clippy gate"
    );
    let largest = call.max(install);
    assert!(initial >= largest);
    assert!(
        initial <= largest + std::mem::align_of::<InitialRecord>(),
        "unexpected enum stack overhead"
    );
}
#[test]
fn one_checked_scoped_layer_preserves_point_request_capacity() {
    let token = token();
    for request in [
        Request::Commit(CommitControl {
            token,
            force: false,
            message: None,
        }),
        Request::Status(token),
        Request::Unmount(UnmountControl {
            token,
            discard: false,
        }),
        Request::Attach(token),
        Request::Locate(token.workspace),
        Request::Cleanup(token),
    ] {
        let ordinary = Call {
            id: 1,
            request: request.clone(),
        }
        .encode()
        .unwrap();
        let observed = Call {
            id: 1,
            request: Request::Observed {
                scope: [7; 32],
                request: Box::new(request),
                traced: false,
            },
        };
        let bytes = observed.encode().unwrap();
        let mut expected = ordinary.clone();
        expected.insert(13, 12);
        expected.splice(14..14, [7; 32]);
        assert_eq!(bytes, expected);
        assert_eq!(bytes.len(), ordinary.len() + 33);
        if !matches!(
            observed.request.without_observation().unwrap(),
            Request::Locate(_)
        ) {
            assert_eq!(
                bytes.capacity(),
                ordinary.capacity(),
                "point control encoding allocation grew"
            );
        }
        assert_eq!(Call::decode(&bytes).unwrap(), observed);
        for end in 0..bytes.len() {
            assert!(Call::decode(&bytes[..end]).is_err());
        }
    }
    // This expected legacy Status vector is literal field order, not an encoder.
    let mut old = prefix(b"LFSC\x01", 3);
    old.extend_from_slice(&[3; 32]);
    old.extend_from_slice(&7i64.to_be_bytes());
    assert_eq!(
        Call {
            id: 1,
            request: Request::Status(token)
        }
        .encode()
        .unwrap(),
        old
    );
}
#[test]
fn observed_lifecycle_and_nesting_are_rejected_before_inner_decode() {
    let forbidden = [
        Request::EndSession,
        Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: false,
        }),
        Request::Observed {
            scope: [7; 32],
            request: Box::new(Request::Status(token())),
            traced: false,
        },
    ];
    for operation in forbidden {
        let request = Request::Observed {
            scope: [7; 32],
            request: Box::new(operation),
            traced: false,
        };
        assert!(request.without_observation().is_err());
        assert!(Call { id: 1, request }.encode().is_err());
    }
    for inner in [7, 8, 12] {
        for unrelated_bytes in [1, 4096] {
            let mut bytes = prefix(b"LFSC\x01", 12);
            bytes.extend_from_slice(&[7; 32]);
            bytes.push(inner);
            bytes.extend(std::iter::repeat_n(12, unrelated_bytes));
            assert_eq!(
                Call::decode(&bytes).unwrap_err().0,
                "invalid observed operation"
            );
        }
    }
}
#[test]
fn zero_scope_is_rejected_before_inner_body_decode() {
    let request = Request::Observed {
        scope: [0; 32],
        request: Box::new(Request::Status(token())),
        traced: false,
    };
    assert_eq!(
        request.without_observation().unwrap_err().0,
        "zero observation scope"
    );
    assert!(Call { id: 1, request }.encode().is_err());
    let mut bytes = prefix(b"LFSC\x01", 12);
    bytes.extend_from_slice(&[0; 32]);
    bytes.extend(std::iter::repeat_n(12, 4096));
    assert_eq!(
        Call::decode(&bytes).unwrap_err().0,
        "zero observation scope"
    );
}
#[test]
fn cleanup_has_exact_additive_tags_and_checked_state() {
    let token = token();
    let mut expected = prefix(b"LFSC\x01", 13);
    expected.extend_from_slice(&[3; 32]);
    expected.extend_from_slice(&7i64.to_be_bytes());
    let call = Call {
        id: 1,
        request: Request::Cleanup(token),
    };
    assert_eq!(call.encode().unwrap(), expected);
    assert_eq!(Call::decode(&expected).unwrap(), call);
    for (tag, state) in [
        (1, CleanupObservation::Live),
        (2, CleanupObservation::Held),
        (3, CleanupObservation::Queued),
        (4, CleanupObservation::Gone),
    ] {
        let answer = Answer {
            id: 1,
            reply: Reply::Cleanup { token, state },
        };
        let mut expected = prefix(b"LFSA\x02", 16);
        expected.extend_from_slice(&[3; 32]);
        expected.extend_from_slice(&7i64.to_be_bytes());
        expected.push(tag);
        assert_eq!(answer.encode().unwrap(), expected);
        assert_eq!(Answer::decode(&expected).unwrap(), answer);
        *expected.last_mut().unwrap() = 0;
        assert!(Answer::decode(&expected).is_err());
    }
}
#[test]
fn traced_observation_differs_only_in_its_prefix_tag() {
    let request = Request::Status(token());
    let plain = Call {
        id: 1,
        request: Request::Observed {
            scope: [7; 32],
            request: Box::new(request.clone()),
            traced: false,
        },
    };
    let traced = Call {
        id: 1,
        request: Request::Observed {
            scope: [7; 32],
            request: Box::new(request),
            traced: true,
        },
    };
    let (plain_bytes, traced_bytes) = (plain.encode().unwrap(), traced.encode().unwrap());
    assert_eq!(plain_bytes[13], 12);
    assert_eq!(traced_bytes[13], 21);
    assert_eq!(plain_bytes[14..], traced_bytes[14..]);
    assert_eq!(Call::decode(&traced_bytes).unwrap(), traced);
    let mut nested = prefix(b"LFSC\x01", 21);
    nested.extend_from_slice(&[7; 32]);
    nested.push(21);
    assert!(Call::decode(&nested).is_err());
}
#[test]
fn observed_reply_carries_telemetry_beside_the_unchanged_reply() {
    use layerfs_bridge::control::SdkValue;
    let inner = Reply::Cleanup {
        token: token(),
        state: CleanupObservation::Gone,
    };
    let plain = Answer {
        id: 1,
        reply: inner.clone(),
    }
    .encode()
    .unwrap();
    let telemetry = SdkValue::map([("timing", SdkValue::List(Vec::new()))]);
    let answer = Answer {
        id: 1,
        reply: Reply::Observed {
            reply: Box::new(inner.clone()),
            telemetry,
        },
    };
    let bytes = answer.encode().unwrap();
    assert_eq!(Answer::decode(&bytes).unwrap(), answer);
    assert_eq!(bytes[..13], plain[..13]);
    assert_eq!(bytes[13], 23);
    assert!(bytes.ends_with(&plain[13..]));
    let nested = Answer {
        id: 1,
        reply: Reply::Observed {
            reply: Box::new(answer.reply.clone()),
            telemetry: SdkValue::Null,
        },
    };
    assert!(nested.encode().is_err());
    for end in 0..bytes.len() {
        assert!(Answer::decode(&bytes[..end]).is_err());
    }
}

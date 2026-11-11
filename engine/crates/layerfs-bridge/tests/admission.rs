//! The advisory pre-handshake capacity notice (#540): one fixed 24-byte body
//! round-trips, and a record that is anything else is never a notice.
use layerfs_bridge::duplex::admission::{
    classify, encode, AdmissionNotice, AdmissionReason, BODY_LEN, MAGIC,
};

#[test]
fn encode_and_classify_round_trip() {
    let notice = AdmissionNotice::new(AdmissionReason::NoPoolSlot, 8, 0, 1);
    let body = encode(&notice);
    assert_eq!(body.len(), BODY_LEN);
    assert_eq!(&body[..16], MAGIC);
    assert_eq!(classify(&body), Some(notice));
}

#[test]
fn garbage_is_not_a_notice() {
    assert_eq!(classify(&[0u8; BODY_LEN]), None);
    assert_eq!(classify(&[0u8; 23]), None);
    let mut body = encode(&AdmissionNotice::new(AdmissionReason::NoPoolSlot, 1, 1, 1));
    body[16] = 2;
    assert_eq!(classify(&body), None);
    let mut body = encode(&AdmissionNotice::new(AdmissionReason::NoPoolSlot, 1, 1, 1));
    body[17] = 0;
    assert_eq!(classify(&body), None);
    let mut body = encode(&AdmissionNotice::new(AdmissionReason::NoPoolSlot, 1, 1, 1));
    body[23] = 1;
    assert_eq!(classify(&body), None);
}

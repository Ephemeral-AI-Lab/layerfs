//! Query authentication separates payload, authority, catalog and incarnation.
use layerfs_history::{query::authenticate_query, CatalogId};
#[test]
fn every_bound_identity_and_payload_changes_the_authenticator() {
    let catalog = CatalogId::derive(b"one").unwrap();
    let key = [7; 32];
    let tag = authenticate_query(&key, catalog, 1, b"query").unwrap();
    assert_eq!(tag, authenticate_query(&key, catalog, 1, b"query").unwrap());
    for other in [
        authenticate_query(&[8; 32], catalog, 1, b"query").unwrap(),
        authenticate_query(&key, CatalogId::derive(b"two").unwrap(), 1, b"query").unwrap(),
        authenticate_query(&key, catalog, 2, b"query").unwrap(),
        authenticate_query(&key, catalog, 1, b"position").unwrap(),
    ] {
        assert_ne!(tag, other);
    }
    assert!(authenticate_query(&key, catalog, 1, &vec![0; 16 * 1024 + 1]).is_err());
}

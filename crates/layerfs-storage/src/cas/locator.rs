//! Persistent FsCas object-locator record and incumbent policy.
//!
//! This module owns the frozen `LFSOBJ01` codec and the semantic binding from
//! one typed physical-object identifier to one sealed carrier index entry.
//! It has no filesystem or publication authority: `cas::fs` performs the
//! fallible open and atomic no-replace installation through the narrow policy
//! seam below, while `cas::locator_index` is only an operation-private
//! rollover lookup.

use crate::format::PhysicalObjectKindV1;
use crate::identity::{ObjectChecksumV1, PackIdV1};
use crate::object::TypedPhysicalObjectIdV1;
use crate::pack::{PackIndexEntryV1, SealedPackV1};

pub const PERSISTENT_LOCATOR_BYTES_V1: usize = 160;

const PERSISTENT_LOCATOR_MAGIC_V1: &[u8; 8] = b"LFSOBJ01";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PersistentObjectLocatorV1 {
    sealed: SealedPackV1,
    entry: PackIndexEntryV1,
    transaction: u64,
}

/// A locator transaction tag is deliberately still serialized as one u64 in
/// the frozen record. Its value is derived from the durable root generation,
/// the current opened-owner incarnation, and the operation-local nonce. It is
/// useful as a diagnostic/filter identity, but it is not deletion custody: a
/// rollback must present the exact operation-local publication receipt before
/// it may unlink a locator.
pub(super) fn locator_transaction_tag_v1(
    generation: [u8; 32],
    incarnation: u64,
    operation_nonce: u64,
) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"LFSLOCAT");
    hasher.update(&generation);
    hasher.update(&incarnation.to_be_bytes());
    hasher.update(&operation_nonce.to_be_bytes());
    u64::from_be_bytes(
        hasher.finalize().as_bytes()[..8]
            .try_into()
            .expect("BLAKE3 digest has at least eight bytes"),
    )
}

impl PersistentObjectLocatorV1 {
    pub(super) const fn new(
        sealed: SealedPackV1,
        entry: PackIndexEntryV1,
        transaction: u64,
    ) -> Self {
        Self {
            sealed,
            entry,
            transaction,
        }
    }

    pub(super) const fn sealed(self) -> SealedPackV1 {
        self.sealed
    }

    pub(super) const fn entry(self) -> PackIndexEntryV1 {
        self.entry
    }

    pub(super) const fn transaction(self) -> u64 {
        self.transaction
    }
}

/// Evidence used before rollback can unlink an object locator. A transaction
/// tag alone is not sufficient custody: a restarted owner may reuse the same
/// numeric nonce, while the locator's sealed carrier and complete index entry
/// identify the exact publication this rollback is currently cleaning up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PersistentLocatorPublicationEvidenceV1 {
    locator: PersistentObjectLocatorV1,
    sealed: SealedPackV1,
    entry: PackIndexEntryV1,
    transaction: u64,
}

impl PersistentLocatorPublicationEvidenceV1 {
    pub(super) const fn new(
        locator: PersistentObjectLocatorV1,
        sealed: SealedPackV1,
        entry: PackIndexEntryV1,
        transaction: u64,
    ) -> Self {
        Self {
            locator,
            sealed,
            entry,
            transaction,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorPublicationDecisionV1 {
    Authenticated,
    Foreign,
}

/// Authenticate rollback custody against the exact carrier/index publication
/// being enumerated. This is deliberately stronger than a bare transaction
/// equality so a reused tag cannot authorize unlinking an earlier carrier's
/// canonical locator.
pub(super) fn decide_persistent_locator_publication_v1(
    evidence: PersistentLocatorPublicationEvidenceV1,
) -> PersistentLocatorPublicationDecisionV1 {
    if evidence.locator.transaction() == evidence.transaction
        && evidence.locator.sealed() == evidence.sealed
        && evidence.locator.entry() == evidence.entry
    {
        PersistentLocatorPublicationDecisionV1::Authenticated
    } else {
        PersistentLocatorPublicationDecisionV1::Foreign
    }
}

/// Immutable physical evidence gathered by `cas::fs` immediately before a
/// locator unlink.  The filesystem layer supplies the observed decoded
/// locator, the exact bytes read from the pathname, the immutable snapshot
/// comparison, and the operation transaction it is attempting to clean.  It
/// does not compare any locator-owned field itself: this decision is the
/// single policy seam for current-operation custody.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PersistentLocatorRollbackEvidenceV1 {
    expected: PersistentObjectLocatorV1,
    observed: PersistentObjectLocatorV1,
    observed_bytes: [u8; PERSISTENT_LOCATOR_BYTES_V1],
    snapshot_matches: bool,
    operation_transaction: u64,
}

impl PersistentLocatorRollbackEvidenceV1 {
    pub(super) const fn new(
        expected: PersistentObjectLocatorV1,
        observed: PersistentObjectLocatorV1,
        observed_bytes: [u8; PERSISTENT_LOCATOR_BYTES_V1],
        snapshot_matches: bool,
        operation_transaction: u64,
    ) -> Self {
        Self {
            expected,
            observed,
            observed_bytes,
            snapshot_matches,
            operation_transaction,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorRollbackDecisionV1 {
    Authorized,
    Foreign,
}

/// Decide whether the exact current-operation locator receipt still owns the
/// pathname.  The durable bytes and decoded binding must both match the
/// receipt, the immutable file identity must still match, and the caller's
/// current operation transaction must match the receipt.  A transaction tag
/// is therefore only one conjunct of exact custody, never the deletion
/// authority by itself.
pub(super) fn decide_persistent_locator_rollback_v1(
    evidence: PersistentLocatorRollbackEvidenceV1,
) -> PersistentLocatorRollbackDecisionV1 {
    if evidence.observed == evidence.expected
        && evidence.observed_bytes == encode_persistent_locator_v1(evidence.expected)
        && evidence.snapshot_matches
        && evidence.expected.transaction() == evidence.operation_transaction
    {
        PersistentLocatorRollbackDecisionV1::Authorized
    } else {
        PersistentLocatorRollbackDecisionV1::Foreign
    }
}

/// Authenticated physical evidence gathered by `cas::fs`. The locator module
/// consumes this evidence; it never opens files, takes locks, or samples
/// filesystem faults itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PersistentLocatorBindingEvidenceV1 {
    locator: PersistentObjectLocatorV1,
    catalog: SealedPackV1,
    indexed: PackIndexEntryV1,
}

impl PersistentLocatorBindingEvidenceV1 {
    pub(super) const fn new(
        locator: PersistentObjectLocatorV1,
        catalog: SealedPackV1,
        indexed: PackIndexEntryV1,
    ) -> Self {
        Self {
            locator,
            catalog,
            indexed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorBindingDecisionV1 {
    Authenticated,
    Collision,
}

pub(super) fn decide_persistent_locator_binding_v1(
    evidence: PersistentLocatorBindingEvidenceV1,
) -> PersistentLocatorBindingDecisionV1 {
    let PersistentLocatorBindingEvidenceV1 {
        locator,
        catalog,
        indexed,
    } = evidence;
    if locator.sealed == catalog && locator.entry == indexed {
        PersistentLocatorBindingDecisionV1::Authenticated
    } else {
        PersistentLocatorBindingDecisionV1::Collision
    }
}

/// The locator's sealed-pack identity and shape must authenticate against the
/// durable catalog before filesystem code uses that evidence to open or
/// validate the carrier. This is distinct from catalog-incumbent comparison:
/// an incumbent catalog can be a same-id unequal candidate, while a locator
/// that disagrees with the authoritative catalog is a binding collision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorCatalogBindingDecisionV1 {
    Authenticated,
    Collision,
}

pub(super) fn decide_persistent_locator_catalog_binding_v1(
    locator: PersistentObjectLocatorV1,
    catalog: SealedPackV1,
) -> PersistentLocatorCatalogBindingDecisionV1 {
    if locator.sealed == catalog {
        PersistentLocatorCatalogBindingDecisionV1::Authenticated
    } else {
        PersistentLocatorCatalogBindingDecisionV1::Collision
    }
}

/// Complete incumbent evidence after `cas::fs` has authenticated the
/// catalog, index entry, carrier object, and candidate/incumbent bytes.
/// `candidate` is retained so this policy can compare the candidate object's
/// identity and shape with the canonical indexed object without requiring the
/// candidate carrier to reuse the incumbent's physical offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PersistentLocatorIncumbentEvidenceV1 {
    binding: PersistentLocatorBindingEvidenceV1,
    candidate: PackIndexEntryV1,
    object_bytes_equal: bool,
}

impl PersistentLocatorIncumbentEvidenceV1 {
    pub(super) const fn new(
        locator: PersistentObjectLocatorV1,
        catalog: SealedPackV1,
        indexed: PackIndexEntryV1,
        candidate: PackIndexEntryV1,
        object_bytes_equal: bool,
    ) -> Self {
        Self {
            binding: PersistentLocatorBindingEvidenceV1::new(locator, catalog, indexed),
            candidate,
            object_bytes_equal,
        }
    }
}

/// Locator-owned incumbent policy. Filesystem code may map these typed
/// outcomes to its public/storage error vocabulary, but it cannot reimplement
/// the binding, collision, equality, or no-replace decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorIncumbentDecisionV1 {
    EqualReuse,
    BindingCollision,
    UnequalObject,
}

pub(super) fn decide_persistent_locator_incumbent_v1(
    evidence: PersistentLocatorIncumbentEvidenceV1,
) -> PersistentLocatorIncumbentDecisionV1 {
    if decide_persistent_locator_binding_v1(evidence.binding)
        != PersistentLocatorBindingDecisionV1::Authenticated
        || !same_object_identity_v1(evidence.binding.indexed, evidence.candidate)
    {
        PersistentLocatorIncumbentDecisionV1::BindingCollision
    } else if !evidence.object_bytes_equal {
        PersistentLocatorIncumbentDecisionV1::UnequalObject
    } else {
        PersistentLocatorIncumbentDecisionV1::EqualReuse
    }
}

/// The filesystem layer reports only the result of its physical no-replace
/// transition.  It must not interpret an incumbent as an equal adoption or a
/// collision itself: that distinction depends on the authenticated catalog,
/// index entry, object identity, and complete-byte comparison carried by the
/// evidence above.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorInstallObservationV1 {
    Installed,
    Incumbent(PersistentLocatorIncumbentEvidenceV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorInstallDecisionV1 {
    Installed,
    EqualReuse,
    BindingCollision,
    UnequalObject,
    Malformed,
}

/// Decode the raw incumbent bytes at the locator-owned semantic boundary.
/// Filesystem code may use the returned locator to gather neutral carrier and
/// index evidence, but it must not classify malformed bytes or a binding
/// mismatch itself. Those two cases are durable locator outcomes, not generic
/// pathname observations.
pub(super) fn decode_persistent_locator_for_install_v1(
    bytes: [u8; PERSISTENT_LOCATOR_BYTES_V1],
    expected: TypedPhysicalObjectIdV1,
) -> Result<PersistentObjectLocatorV1, PersistentLocatorInstallDecisionV1> {
    decode_persistent_locator_v1(bytes, expected).map_err(|error| match error {
        PersistentLocatorCodecErrorV1::Malformed => PersistentLocatorInstallDecisionV1::Malformed,
        PersistentLocatorCodecErrorV1::BindingMismatch => {
            PersistentLocatorInstallDecisionV1::BindingCollision
        }
    })
}

/// Decode a locator whose typed object binding is carried by the record
/// itself, as needed by operation-local publication receipts.
pub(super) fn decode_persistent_locator_self_describing_v1(
    bytes: [u8; PERSISTENT_LOCATOR_BYTES_V1],
) -> Result<PersistentObjectLocatorV1, PersistentLocatorCodecErrorV1> {
    let kind = PhysicalObjectKindV1::try_from(bytes[8])
        .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?;
    let digest = bytes[16..48]
        .try_into()
        .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?;
    decode_persistent_locator_v1(
        bytes,
        TypedPhysicalObjectIdV1::from_kind_and_digest(kind, digest),
    )
}

/// Locator-owned interpretation of a physical no-replace publication.  The
/// carrier/locator pathname operation remains in `cas::fs`; this function owns
/// the durable locator meaning of its two neutral observations.
pub(super) fn decide_persistent_locator_install_v1(
    observation: PersistentLocatorInstallObservationV1,
) -> PersistentLocatorInstallDecisionV1 {
    match observation {
        PersistentLocatorInstallObservationV1::Installed => {
            PersistentLocatorInstallDecisionV1::Installed
        }
        PersistentLocatorInstallObservationV1::Incumbent(evidence) => {
            match decide_persistent_locator_incumbent_v1(evidence) {
                PersistentLocatorIncumbentDecisionV1::EqualReuse => {
                    PersistentLocatorInstallDecisionV1::EqualReuse
                }
                PersistentLocatorIncumbentDecisionV1::BindingCollision => {
                    PersistentLocatorInstallDecisionV1::BindingCollision
                }
                PersistentLocatorIncumbentDecisionV1::UnequalObject => {
                    PersistentLocatorInstallDecisionV1::UnequalObject
                }
            }
        }
    }
}

/// A candidate pack may place an already-authenticated object at a different
/// physical offset. Its typed identity, encoded length, and checksum must
/// still match the canonical locator entry before complete bytes can qualify
/// as equal reuse.
fn same_object_identity_v1(canonical: PackIndexEntryV1, candidate: PackIndexEntryV1) -> bool {
    canonical.id() == candidate.id()
        && canonical.object_len() == candidate.object_len()
        && canonical.object_checksum() == candidate.object_checksum()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentCatalogIncumbentDecisionV1 {
    Authenticated,
    Collision,
    Unequal,
}

pub(super) fn decide_persistent_catalog_incumbent_v1(
    incumbent: SealedPackV1,
    expected: SealedPackV1,
) -> PersistentCatalogIncumbentDecisionV1 {
    if incumbent.id() != expected.id() {
        PersistentCatalogIncumbentDecisionV1::Collision
    } else if incumbent == expected {
        PersistentCatalogIncumbentDecisionV1::Authenticated
    } else {
        PersistentCatalogIncumbentDecisionV1::Unequal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorCodecErrorV1 {
    Malformed,
    BindingMismatch,
}

pub(super) fn encode_persistent_locator_v1(
    locator: PersistentObjectLocatorV1,
) -> [u8; PERSISTENT_LOCATOR_BYTES_V1] {
    let mut bytes = [0_u8; PERSISTENT_LOCATOR_BYTES_V1];
    bytes[..8].copy_from_slice(PERSISTENT_LOCATOR_MAGIC_V1);
    bytes[8] = typed_kind_byte(locator.entry.id());
    bytes[16..48].copy_from_slice(locator.entry.id().as_bytes());
    bytes[48..80].copy_from_slice(locator.sealed.id().as_bytes());
    bytes[80..88].copy_from_slice(&locator.sealed.pack_len().to_be_bytes());
    bytes[88..92].copy_from_slice(&locator.sealed.record_count().to_be_bytes());
    bytes[96..104].copy_from_slice(&locator.sealed.index_offset().to_be_bytes());
    bytes[104..112].copy_from_slice(&locator.entry.absolute_offset().to_be_bytes());
    bytes[112..116].copy_from_slice(&locator.entry.object_len().to_be_bytes());
    bytes[120..152].copy_from_slice(locator.entry.object_checksum().as_bytes());
    bytes[152..160].copy_from_slice(&locator.transaction.to_be_bytes());
    bytes
}

pub(super) fn decode_persistent_locator_v1(
    bytes: [u8; PERSISTENT_LOCATOR_BYTES_V1],
    expected: TypedPhysicalObjectIdV1,
) -> Result<PersistentObjectLocatorV1, PersistentLocatorCodecErrorV1> {
    if &bytes[..8] != PERSISTENT_LOCATOR_MAGIC_V1
        || !(1..=5).contains(&bytes[8])
        || bytes[9..16] != [0_u8; 7]
        || bytes[92..96] != [0_u8; 4]
        || bytes[116..120] != [0_u8; 4]
    {
        return Err(PersistentLocatorCodecErrorV1::Malformed);
    }
    if bytes[8] != typed_kind_byte(expected) || bytes[16..48] != *expected.as_bytes() {
        return Err(PersistentLocatorCodecErrorV1::BindingMismatch);
    }
    let pack_id = <[u8; 32]>::try_from(&bytes[48..80])
        .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?;
    let pack_len = u64::from_be_bytes(
        bytes[80..88]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let record_count = u32::from_be_bytes(
        bytes[88..92]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let index_offset = u64::from_be_bytes(
        bytes[96..104]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let absolute_offset = u64::from_be_bytes(
        bytes[104..112]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let object_len = u32::from_be_bytes(
        bytes[112..116]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let checksum = <[u8; 32]>::try_from(&bytes[120..152])
        .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?;
    let transaction = u64::from_be_bytes(
        bytes[152..160]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    Ok(PersistentObjectLocatorV1::new(
        SealedPackV1::from_validated_parts(
            PackIdV1::from_digest(pack_id),
            pack_len,
            record_count,
            index_offset,
        ),
        PackIndexEntryV1::from_validated_parts(
            expected,
            absolute_offset,
            object_len,
            ObjectChecksumV1::from_digest(checksum),
        ),
        transaction,
    ))
}

const fn typed_kind_byte(id: TypedPhysicalObjectIdV1) -> u8 {
    match id {
        TypedPhysicalObjectIdV1::VersionRecord(_) => 1,
        TypedPhysicalObjectIdV1::Tree(_) => 2,
        TypedPhysicalObjectIdV1::File(_) => 3,
        TypedPhysicalObjectIdV1::Symlink(_) => 4,
        TypedPhysicalObjectIdV1::Chunk(_) => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{ObjectChecksumV1, PackIdV1, PhysicalFileIdV1};

    fn fixture() -> (PersistentObjectLocatorV1, TypedPhysicalObjectIdV1) {
        let id = TypedPhysicalObjectIdV1::File(PhysicalFileIdV1::from_digest([0x11; 32]));
        let sealed = SealedPackV1::from_validated_parts(
            PackIdV1::from_digest([0x22; 32]),
            0x0102_0304_0506_0708,
            0x090a_0b0c,
            0x1112_1314_1516_1718,
        );
        let entry = PackIndexEntryV1::from_validated_parts(
            id,
            0x2122_2324_2526_2728,
            0x3132_3334,
            ObjectChecksumV1::from_digest([0x44; 32]),
        );
        (
            PersistentObjectLocatorV1::new(sealed, entry, 0x5152_5354_5556_5758),
            id,
        )
    }

    #[test]
    fn persistent_locator_frozen_bytes_round_trip_exactly() {
        let (locator, id) = fixture();
        let encoded = encode_persistent_locator_v1(locator);
        assert_eq!(&encoded[..8], b"LFSOBJ01");
        assert_eq!(encoded[8], 3);
        assert_eq!(&encoded[9..16], &[0; 7]);
        assert_eq!(&encoded[16..48], &[0x11; 32]);
        assert_eq!(&encoded[48..80], &[0x22; 32]);
        assert_eq!(&encoded[80..88], &0x0102_0304_0506_0708_u64.to_be_bytes());
        assert_eq!(&encoded[88..92], &0x090a_0b0c_u32.to_be_bytes());
        assert_eq!(&encoded[92..96], &[0; 4]);
        assert_eq!(&encoded[96..104], &0x1112_1314_1516_1718_u64.to_be_bytes());
        assert_eq!(&encoded[104..112], &0x2122_2324_2526_2728_u64.to_be_bytes());
        assert_eq!(&encoded[112..116], &0x3132_3334_u32.to_be_bytes());
        assert_eq!(&encoded[116..120], &[0; 4]);
        assert_eq!(&encoded[120..152], &[0x44; 32]);
        assert_eq!(&encoded[152..160], &0x5152_5354_5556_5758_u64.to_be_bytes());
        assert_eq!(decode_persistent_locator_v1(encoded, id), Ok(locator));
        assert_eq!(
            decide_persistent_locator_binding_v1(PersistentLocatorBindingEvidenceV1::new(
                locator,
                locator.sealed(),
                locator.entry(),
            )),
            PersistentLocatorBindingDecisionV1::Authenticated
        );
        assert_eq!(
            decide_persistent_locator_publication_v1(PersistentLocatorPublicationEvidenceV1::new(
                locator,
                locator.sealed(),
                locator.entry(),
                locator.transaction(),
            ),),
            PersistentLocatorPublicationDecisionV1::Authenticated
        );
    }

    #[test]
    fn persistent_locator_distinguishes_structure_from_typed_binding() {
        let (locator, id) = fixture();
        let mut encoded = encode_persistent_locator_v1(locator);
        encoded[9] = 1;
        assert_eq!(
            decode_persistent_locator_v1(encoded, id),
            Err(PersistentLocatorCodecErrorV1::Malformed)
        );

        let encoded = encode_persistent_locator_v1(locator);
        let wrong = TypedPhysicalObjectIdV1::Chunk(
            crate::identity::PhysicalChunkIdV1::from_digest([0x11; 32]),
        );
        assert_eq!(
            decode_persistent_locator_v1(encoded, wrong),
            Err(PersistentLocatorCodecErrorV1::BindingMismatch)
        );

        let mut wrong_id = encode_persistent_locator_v1(locator);
        wrong_id[16] ^= 0xff;
        assert_eq!(
            decode_persistent_locator_v1(wrong_id, id),
            Err(PersistentLocatorCodecErrorV1::BindingMismatch)
        );

        let mut unknown_kind = encode_persistent_locator_v1(locator);
        unknown_kind[8] = 0xff;
        assert_eq!(
            decode_persistent_locator_v1(unknown_kind, id),
            Err(PersistentLocatorCodecErrorV1::Malformed)
        );
    }

    #[test]
    fn self_describing_locator_rejects_wrong_magic_and_version() {
        let (locator, _) = fixture();
        let mut wrong_magic = encode_persistent_locator_v1(locator);
        wrong_magic[0] ^= 0xff;
        assert_eq!(
            decode_persistent_locator_self_describing_v1(wrong_magic),
            Err(PersistentLocatorCodecErrorV1::Malformed)
        );

        let mut wrong_version = encode_persistent_locator_v1(locator);
        wrong_version[7] = b'2';
        assert_eq!(
            decode_persistent_locator_self_describing_v1(wrong_version),
            Err(PersistentLocatorCodecErrorV1::Malformed)
        );
    }

    #[test]
    fn persistent_locator_binding_policy_classifies_pack_and_object_collisions() {
        let (locator, id) = fixture();
        let original_pack = locator.sealed();
        let wrong_pack = SealedPackV1::from_validated_parts(
            PackIdV1::from_digest([0x99; 32]),
            original_pack.pack_len(),
            original_pack.record_count(),
            original_pack.index_offset(),
        );
        assert_eq!(
            decide_persistent_locator_catalog_binding_v1(locator, original_pack),
            PersistentLocatorCatalogBindingDecisionV1::Authenticated
        );
        assert_eq!(
            decide_persistent_locator_catalog_binding_v1(locator, wrong_pack),
            PersistentLocatorCatalogBindingDecisionV1::Collision
        );
        assert_eq!(
            decide_persistent_locator_binding_v1(PersistentLocatorBindingEvidenceV1::new(
                locator,
                wrong_pack,
                locator.entry(),
            )),
            PersistentLocatorBindingDecisionV1::Collision
        );

        let wrong_entry = PackIndexEntryV1::from_validated_parts(
            id,
            locator.entry().absolute_offset() + 1,
            locator.entry().object_len(),
            locator.entry().object_checksum(),
        );
        assert_eq!(
            decide_persistent_locator_binding_v1(PersistentLocatorBindingEvidenceV1::new(
                locator,
                locator.sealed(),
                wrong_entry,
            )),
            PersistentLocatorBindingDecisionV1::Collision
        );
        assert_eq!(
            decide_persistent_locator_incumbent_v1(PersistentLocatorIncumbentEvidenceV1::new(
                locator,
                locator.sealed(),
                locator.entry(),
                locator.entry(),
                true,
            )),
            PersistentLocatorIncumbentDecisionV1::EqualReuse
        );
        assert_eq!(
            decide_persistent_locator_incumbent_v1(PersistentLocatorIncumbentEvidenceV1::new(
                locator,
                locator.sealed(),
                locator.entry(),
                wrong_entry,
                true,
            )),
            PersistentLocatorIncumbentDecisionV1::EqualReuse
        );
        let wrong_shape_entry = PackIndexEntryV1::from_validated_parts(
            id,
            locator.entry().absolute_offset() + 1,
            locator.entry().object_len() + 1,
            locator.entry().object_checksum(),
        );
        assert_eq!(
            decide_persistent_locator_incumbent_v1(PersistentLocatorIncumbentEvidenceV1::new(
                locator,
                locator.sealed(),
                locator.entry(),
                wrong_shape_entry,
                true,
            )),
            PersistentLocatorIncumbentDecisionV1::BindingCollision
        );
        assert_eq!(
            decide_persistent_locator_incumbent_v1(PersistentLocatorIncumbentEvidenceV1::new(
                locator,
                locator.sealed(),
                locator.entry(),
                locator.entry(),
                false,
            )),
            PersistentLocatorIncumbentDecisionV1::UnequalObject
        );
    }

    #[test]
    fn persistent_locator_install_policy_owns_all_physical_observations() {
        let (locator, id) = fixture();
        let equal = PersistentLocatorIncumbentEvidenceV1::new(
            locator,
            locator.sealed(),
            locator.entry(),
            locator.entry(),
            true,
        );
        let wrong_shape = PersistentLocatorIncumbentEvidenceV1::new(
            locator,
            locator.sealed(),
            locator.entry(),
            PackIndexEntryV1::from_validated_parts(
                id,
                locator.entry().absolute_offset() + 1,
                locator.entry().object_len() + 1,
                locator.entry().object_checksum(),
            ),
            true,
        );
        let unequal_bytes = PersistentLocatorIncumbentEvidenceV1::new(
            locator,
            locator.sealed(),
            locator.entry(),
            locator.entry(),
            false,
        );

        assert_eq!(
            decide_persistent_locator_install_v1(PersistentLocatorInstallObservationV1::Installed),
            PersistentLocatorInstallDecisionV1::Installed
        );
        assert_eq!(
            decide_persistent_locator_install_v1(PersistentLocatorInstallObservationV1::Incumbent(
                equal
            )),
            PersistentLocatorInstallDecisionV1::EqualReuse
        );
        assert_eq!(
            decide_persistent_locator_install_v1(PersistentLocatorInstallObservationV1::Incumbent(
                wrong_shape
            )),
            PersistentLocatorInstallDecisionV1::BindingCollision
        );
        assert_eq!(
            decide_persistent_locator_install_v1(PersistentLocatorInstallObservationV1::Incumbent(
                unequal_bytes
            )),
            PersistentLocatorInstallDecisionV1::UnequalObject
        );

        assert_eq!(
            decode_persistent_locator_for_install_v1([0; PERSISTENT_LOCATOR_BYTES_V1], id),
            Err(PersistentLocatorInstallDecisionV1::Malformed)
        );
        let mut foreign_bytes = encode_persistent_locator_v1(locator);
        foreign_bytes[16] ^= 1;
        assert_eq!(
            decode_persistent_locator_for_install_v1(foreign_bytes, id),
            Err(PersistentLocatorInstallDecisionV1::BindingCollision)
        );
    }

    #[test]
    fn locator_transaction_tag_rejects_reused_nonce_across_incarnation_and_root() {
        let generation = [0x71; 32];
        let other_generation = [0x72; 32];
        let first = locator_transaction_tag_v1(generation, 7, 1);
        let different_operation = locator_transaction_tag_v1(generation, 7, 2);
        assert_eq!(first, locator_transaction_tag_v1(generation, 7, 1));
        assert_ne!(
            first, different_operation,
            "a different operation nonce must not own the earlier locator"
        );
        assert_ne!(
            first,
            locator_transaction_tag_v1(generation, 8, 1),
            "a reopened incarnation must not reuse the earlier locator tag"
        );
        assert_ne!(
            first,
            locator_transaction_tag_v1(other_generation, 7, 1),
            "a different durable root generation must not share locator tags"
        );
        let (fixture_locator, _) = fixture();
        let scoped = PersistentObjectLocatorV1::new(
            fixture_locator.sealed(),
            fixture_locator.entry(),
            first,
        );
        let publication = |transaction| {
            decide_persistent_locator_publication_v1(PersistentLocatorPublicationEvidenceV1::new(
                scoped,
                scoped.sealed(),
                scoped.entry(),
                transaction,
            ))
        };
        assert_eq!(
            publication(first),
            PersistentLocatorPublicationDecisionV1::Authenticated
        );
        assert_eq!(
            publication(different_operation),
            PersistentLocatorPublicationDecisionV1::Foreign
        );
        assert_eq!(
            publication(locator_transaction_tag_v1(generation, 8, 1)),
            PersistentLocatorPublicationDecisionV1::Foreign,
            "a wrong reopened incarnation is foreign custody"
        );
        assert_eq!(
            publication(locator_transaction_tag_v1(other_generation, 7, 1)),
            PersistentLocatorPublicationDecisionV1::Foreign,
            "a wrong durable root generation is foreign custody"
        );
        assert_eq!(
            decide_persistent_locator_publication_v1(PersistentLocatorPublicationEvidenceV1::new(
                scoped,
                scoped.sealed(),
                scoped.entry(),
                first,
            ),),
            PersistentLocatorPublicationDecisionV1::Authenticated
        );
        assert_eq!(
            decide_persistent_locator_publication_v1(PersistentLocatorPublicationEvidenceV1::new(
                scoped,
                SealedPackV1::from_validated_parts(
                    PackIdV1::from_digest([0x99; 32]),
                    scoped.sealed().pack_len(),
                    scoped.sealed().record_count(),
                    scoped.sealed().index_offset(),
                ),
                scoped.entry(),
                first,
            ),),
            PersistentLocatorPublicationDecisionV1::Foreign
        );
        assert_eq!(
            decide_persistent_locator_publication_v1(PersistentLocatorPublicationEvidenceV1::new(
                scoped,
                scoped.sealed(),
                PackIndexEntryV1::from_validated_parts(
                    scoped.entry().id(),
                    scoped.entry().absolute_offset() + 1,
                    scoped.entry().object_len(),
                    scoped.entry().object_checksum(),
                ),
                first,
            ),),
            PersistentLocatorPublicationDecisionV1::Foreign
        );
    }

    #[test]
    fn locator_rollback_policy_requires_exact_receipt_and_current_operation() {
        let (locator, _) = fixture();
        let bytes = encode_persistent_locator_v1(locator);
        let evidence = |observed, observed_bytes, snapshot_matches, operation_transaction| {
            decide_persistent_locator_rollback_v1(PersistentLocatorRollbackEvidenceV1::new(
                locator,
                observed,
                observed_bytes,
                snapshot_matches,
                operation_transaction,
            ))
        };

        assert_eq!(
            evidence(locator, bytes, true, locator.transaction()),
            PersistentLocatorRollbackDecisionV1::Authorized
        );
        assert_eq!(
            evidence(locator, bytes, true, locator.transaction().wrapping_add(1)),
            PersistentLocatorRollbackDecisionV1::Foreign,
            "the current operation is part of locator deletion custody"
        );
        let foreign_locator = PersistentObjectLocatorV1::new(
            locator.sealed(),
            PackIndexEntryV1::from_validated_parts(
                locator.entry().id(),
                locator.entry().absolute_offset() + 1,
                locator.entry().object_len(),
                locator.entry().object_checksum(),
            ),
            locator.transaction(),
        );
        assert_eq!(
            evidence(foreign_locator, bytes, true, locator.transaction()),
            PersistentLocatorRollbackDecisionV1::Foreign
        );
        let mut foreign_bytes = bytes;
        foreign_bytes[152] ^= 1;
        assert_eq!(
            evidence(locator, foreign_bytes, true, locator.transaction()),
            PersistentLocatorRollbackDecisionV1::Foreign
        );
        assert_eq!(
            evidence(locator, bytes, false, locator.transaction()),
            PersistentLocatorRollbackDecisionV1::Foreign
        );
    }
}

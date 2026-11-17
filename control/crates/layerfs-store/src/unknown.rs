//! Spec §10 A6: the `Unknown` detail a GC call can name when the worker's
//! answer was lost. A lost reply is `refused("Unknown", ...)` with a null
//! detail; the caller still knows the entity it addressed, and the operation
//! identity when it held one.
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// Names the entity on an `Unknown` refusal whose detail carries nothing.
///
/// A reply lost on the transport has a null detail; a bare `UnknownOutcome`
/// a call cannot name yet renders the empty map from the store service's
/// `lookup::history` (spec gap G1). Both mean "no detail", so the caller
/// fills them. A refusal that already names something, or any other code, is
/// returned unchanged. `phase` stays null: the lost call did not report one,
/// and `op` is the durable operation the caller held, if any.
pub(crate) fn named_unknown(
    mut refusal: SdkRefusal,
    kind: &str,
    name: &str,
    op: Option<&[u8; 16]>,
) -> SdkRefusal {
    if refusal.code != "Unknown" || !no_detail(&refusal.detail) {
        return refusal;
    }
    refusal.detail = SdkValue::map([
        (
            "entity",
            SdkValue::map([
                ("kind", SdkValue::text(kind)),
                ("name", SdkValue::text(name)),
            ]),
        ),
        ("phase", SdkValue::Null),
        (
            "op",
            op.map_or(SdkValue::Null, |op| {
                SdkValue::text(layerfs_bridge::sdk_values::hex(op))
            }),
        ),
    ]);
    refusal
}

/// Whether a detail carries no fields: a lost answer's null, or the empty map
/// a bare `UnknownOutcome` renders.
fn no_detail(detail: &SdkValue) -> bool {
    *detail == SdkValue::Null || matches!(detail, SdkValue::Map(fields) if fields.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(code: &str, detail: SdkValue) -> SdkRefusal {
        SdkRefusal {
            code: code.to_owned(),
            message: "the worker did not answer".to_owned(),
            detail,
        }
    }

    fn named_store(refusal: &SdkRefusal) {
        let entity = refusal.detail.get("entity").expect("entity");
        assert_eq!(entity.get_text("kind"), Some("store"), "{refusal:?}");
        assert_eq!(entity.get_text("name"), Some("shop"), "{refusal:?}");
        assert!(
            refusal.detail.get("phase").expect("phase") == &SdkValue::Null,
            "{refusal:?}"
        );
    }

    #[test]
    fn a_null_detail_is_filled_with_the_entity() {
        let refusal = named_unknown(refused("Unknown", SdkValue::Null), "store", "shop", None);
        assert_eq!(refusal.code, "Unknown");
        named_store(&refusal);
        assert!(refusal.detail.get("op").expect("op") == &SdkValue::Null);
    }

    /// F43R4-1: a bare `UnknownOutcome` renders `SdkValue::map::<0>([])`, not
    /// `Null`; the caller must fill that shape too.
    #[test]
    fn an_empty_map_detail_is_filled_with_the_entity_and_operation() {
        let op = [0xab; 16];
        let refusal = named_unknown(
            refused("Unknown", SdkValue::map::<0>([])),
            "store",
            "shop",
            Some(&op),
        );
        assert_eq!(refusal.code, "Unknown");
        named_store(&refusal);
        let expected = "ab".repeat(16);
        assert_eq!(refusal.detail.get_text("op"), Some(expected.as_str()));
    }

    #[test]
    fn a_detail_that_already_names_something_is_kept() {
        let detail = SdkValue::map([(
            "entity",
            SdkValue::map([
                ("kind", SdkValue::text("store")),
                ("name", SdkValue::text("other")),
            ]),
        )]);
        let refusal = named_unknown(refused("Unknown", detail.clone()), "store", "shop", None);
        assert_eq!(refusal.detail, detail);
    }

    #[test]
    fn a_definite_refusal_is_kept() {
        let refusal = named_unknown(
            refused("Busy", SdkValue::map::<0>([])),
            "store",
            "shop",
            None,
        );
        assert_eq!(refusal.code, "Busy");
        assert!(matches!(refusal.detail, SdkValue::Map(fields) if fields.is_empty()));
    }
}

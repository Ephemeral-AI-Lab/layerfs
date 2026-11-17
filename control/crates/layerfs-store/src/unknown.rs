//! Spec §10 A6: the `Unknown` detail a GC call can name when the worker's
//! answer was lost. A lost reply is `refused("Unknown", ...)` with a null
//! detail; the caller still knows the entity it addressed, and the operation
//! identity when it held one.
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// Names the entity on an `Unknown` refusal whose detail is null (the worker's
/// answer was lost). A refusal that already carries a detail, or any other
/// code, is returned unchanged. `phase` stays null: the lost call did not
/// report one, and `op` is the durable operation the caller held, if any.
pub(crate) fn named_unknown(
    mut refusal: SdkRefusal,
    kind: &str,
    name: &str,
    op: Option<&[u8; 16]>,
) -> SdkRefusal {
    if refusal.code != "Unknown" || refusal.detail != SdkValue::Null {
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

//! Spec §10 A6: the `Unknown` detail a GC call can name when the worker's
//! answer was lost. A lost reply is `refused("Unknown", ...)` with a null
//! detail; the caller still knows the entity it addressed, and the operation
//! identity when it held one.
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

/// Names the entity on an `Unknown` refusal whose detail carries nothing.
///
/// A reply lost on the transport has a null detail; a bare `UnknownOutcome`
/// a call cannot name yet renders the empty map from the store service's
/// `lookup::history` (spec gap G1). Both mean "no detail", so the caller
/// fills them. A refusal that already names something, or any other code, is
/// returned unchanged. `phase` stays null: the lost call did not report one,
/// and `op` is the durable operation the caller held, if any.
#[doc(hidden)]
pub fn named_unknown(
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
                SdkValue::text(layerfs_protocol::sdk::hex(op))
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

//! A refusal at the call boundary (call surface 2.0): one of the thirteen
//! public codes, a message, a `fix` when there is one, and the fields the
//! caller's next call needs, flat beside them. The control and engine
//! crates keep their own codes; the dispatcher folds each into its public
//! one here, and every front end prints or raises the same map.
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::home::with_field;
use layerfs_home::outcome::{public_code, FIX, NEXT};

/// `refusal` as the dispatcher answers it: its code one of the public
/// thirteen. A code that was folded stays readable as the long form's
/// `detail.code`.
pub fn public(mut refusal: SdkRefusal) -> SdkRefusal {
    let code = public_code(&refusal.code);
    if code != refusal.code {
        let internal = std::mem::replace(&mut refusal.code, code.to_owned());
        if refusal.detail.get("code").is_none() {
            refusal.detail = with_field(refusal.detail, "code", SdkValue::Text(internal));
        }
    }
    refusal
}

/// `refusal` as a front end prints or raises it: `{ error, message, fix }`
/// and the fields its next call needs, flat. The short form leaves `fix`
/// out when there is none; the long form (`detail`) always has it, and adds
/// `detail`: what else the refusing code recorded.
pub fn value(refusal: &SdkRefusal, detail: bool) -> SdkValue {
    let mut fields = vec![
        (
            "error".to_owned(),
            SdkValue::text(public_code(&refusal.code)),
        ),
        ("message".to_owned(), SdkValue::text(&refusal.message)),
    ];
    let fix = refusal.detail.get(FIX).cloned().unwrap_or_default();
    if detail || fix != SdkValue::Null {
        fields.push((FIX.to_owned(), fix));
    }
    if let Some(SdkValue::Map(next)) = refusal.detail.get(NEXT) {
        let taken = |name: &str| fields.iter().any(|(held, _)| held == name) || name == "detail";
        let next: Vec<_> = next.iter().filter(|(name, _)| !taken(name)).collect();
        fields.extend(next.into_iter().cloned());
    }
    if detail {
        let rest = match &refusal.detail {
            SdkValue::Map(held) => {
                let rest: Vec<_> = (held.iter())
                    .filter(|(name, _)| name != FIX && name != NEXT)
                    .cloned()
                    .collect();
                match rest.is_empty() {
                    true => SdkValue::Null,
                    false => SdkValue::Map(rest),
                }
            }
            other => other.clone(),
        };
        fields.push(("detail".to_owned(), rest));
    }
    SdkValue::Map(fields)
}

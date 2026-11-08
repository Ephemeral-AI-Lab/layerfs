//! Store-wide publication counters, read without walking history or objects.
use crate::{history::lookup::Answer, StoreService};
use layerfs_bridge::control::SdkValue;

pub fn stats(service: &StoreService) -> Answer {
    let value = crate::operator::store::stats(&service.store)?;
    let SdkValue::Map(fields) = value else {
        unreachable!()
    };
    Ok(SdkValue::Map(
        fields
            .into_iter()
            .filter_map(|(name, value)| match name.as_str() {
                "dedupe" | "compression" => None,
                "logical" => Some(("historyLogical".to_owned(), value)),
                _ => Some((name, value)),
            })
            .collect(),
    ))
}

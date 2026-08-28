#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::stage1_materialize) enum AttributionArm {
    Complete,
    Null,
    Digest,
    Native,
}

impl AttributionArm {
    pub(in crate::stage1_materialize) fn parse(value: &OsStr) -> EvalResult<Self> {
        match value.to_str() {
            Some("complete") => Ok(Self::Complete),
            Some("null") => Ok(Self::Null),
            Some("digest") => Ok(Self::Digest),
            Some("native") => Ok(Self::Native),
            _ => Err("arm must be complete, null, digest, or native".to_owned()),
        }
    }

    pub(in crate::stage1_materialize) const fn name(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Null => "null",
            Self::Digest => "digest",
            Self::Native => "native",
        }
    }

    pub(in crate::stage1_materialize) const fn operation_label(self) -> &'static str {
        match self {
            Self::Complete => "same_open_warmed_source_fresh_destination",
            Self::Null => "warm_authenticated_null_sink",
            Self::Digest => "warm_authenticated_digest",
            Self::Native => "native_durable_output",
        }
    }
}

pub(in crate::stage1_materialize) const ATTRIBUTION_SCHEDULE: [(AttributionArm, u64); 12] = [
    (AttributionArm::Complete, 24),
    (AttributionArm::Null, 0),
    (AttributionArm::Digest, 96),
    (AttributionArm::Native, 24),
    (AttributionArm::Null, 96),
    (AttributionArm::Digest, 24),
    (AttributionArm::Native, 0),
    (AttributionArm::Complete, 96),
    (AttributionArm::Digest, 0),
    (AttributionArm::Native, 96),
    (AttributionArm::Complete, 0),
    (AttributionArm::Null, 24),
];

pub(in crate::stage1_materialize) fn attribution_schedule_json() -> String {
    let blocks = ATTRIBUTION_SCHEDULE
        .iter()
        .enumerate()
        .map(|(index, (arm, size))| {
            format!(
                "{{\"block\":{},\"arm\":\"{}\",\"size_mib\":{},\"warmups\":1,\"measured\":3}}",
                index + 1,
                arm.name(),
                size
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":\"layerfs-stage1m-attribution-schedule-v1\",\"blocks\":[{blocks}],\"warmups\":12,\"measured\":36,\"rows\":48}}\n"
    )
}
use super::super::contract::EvalResult;
use std::ffi::OsStr;

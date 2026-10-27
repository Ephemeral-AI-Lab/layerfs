//! Persistent history facts and constant-size Store accounting reads.
use super::{provider::HistoryProvider, rows::cell};
use crate::backend::records::Param;
use layerfs_content::merge::{LwwDecision, MergeUnit};
use layerfs_history::{HistoryError, HistoryResult, InitStatistics, StoreStatistics};

/// Marks the merge units after the LWW decisions: no path is that long, so
/// a blob written before units reads as having none (#352 F-057).
const UNITS: [u8; 4] = [0xFF; 4];
fn put(out: &mut Vec<u8>, count: usize) -> HistoryResult<()> {
    let count = u32::try_from(count)
        .ok()
        .filter(|count| *count != u32::MAX)
        .ok_or(HistoryError::Capacity("history fact text"))?;
    out.extend_from_slice(&count.to_be_bytes());
    Ok(())
}
pub(crate) fn encode(values: &[LwwDecision], units: &[MergeUnit]) -> HistoryResult<Vec<u8>> {
    let mut out = Vec::new();
    for value in values {
        for text in [&value.path, &value.rule] {
            put(&mut out, text.len())?;
            out.extend_from_slice(text.as_bytes());
        }
        out.push(u8::from(value.kept_ours) | (u8::from(value.deleted) << 1));
    }
    if !units.is_empty() {
        out.extend_from_slice(&UNITS);
        put(&mut out, units.len())?;
        for unit in units {
            put(&mut out, unit.name.len())?;
            out.extend_from_slice(unit.name.as_bytes());
            put(&mut out, unit.patterns.len())?;
            for pattern in &unit.patterns {
                put(&mut out, pattern.len())?;
                out.extend_from_slice(pattern.as_bytes());
            }
        }
    }
    Ok(out)
}
type Facts = (Vec<LwwDecision>, Vec<MergeUnit>);
pub(crate) fn decode(mut bytes: &[u8]) -> HistoryResult<Facts> {
    fn count(bytes: &mut &[u8]) -> HistoryResult<usize> {
        let count = bytes
            .get(..4)
            .ok_or(HistoryError::Integrity("history fact length"))?;
        let count = u32::from_be_bytes(
            count
                .try_into()
                .map_err(|_| HistoryError::Integrity("history fact length"))?,
        ) as usize;
        *bytes = &bytes[4..];
        Ok(count)
    }
    fn text(bytes: &mut &[u8]) -> HistoryResult<String> {
        let count = bytes
            .get(..4)
            .ok_or(HistoryError::Integrity("history fact length"))?;
        let length = u32::from_be_bytes(
            count
                .try_into()
                .map_err(|_| HistoryError::Integrity("history fact length"))?,
        ) as usize;
        *bytes = &bytes[4..];
        let text = bytes
            .get(..length)
            .ok_or(HistoryError::Integrity("history fact text"))?;
        let out = std::str::from_utf8(text)
            .map_err(|_| HistoryError::Integrity("history fact UTF-8"))?
            .to_owned();
        *bytes = &bytes[length..];
        Ok(out)
    }
    let mut out = Vec::new();
    let mut units = Vec::new();
    while !bytes.is_empty() {
        if bytes.starts_with(&UNITS) {
            bytes = &bytes[4..];
            for _ in 0..count(&mut bytes)? {
                let name = text(&mut bytes)?;
                let patterns = (0..count(&mut bytes)?)
                    .map(|_| text(&mut bytes))
                    .collect::<HistoryResult<_>>()?;
                units.push(MergeUnit { name, patterns });
            }
            if !bytes.is_empty() || units.is_empty() {
                return Err(HistoryError::Integrity("history fact units"));
            }
            break;
        }
        let path = text(&mut bytes)?;
        let rule = text(&mut bytes)?;
        let flags = *bytes
            .first()
            .ok_or(HistoryError::Integrity("history fact flags"))?;
        if flags > 3 {
            return Err(HistoryError::Integrity("history fact flags"));
        }
        bytes = &bytes[1..];
        out.push(LwwDecision {
            path,
            rule,
            kept_ours: flags & 1 != 0,
            deleted: flags & 2 != 0,
        });
    }
    Ok((out, units))
}
impl HistoryProvider {
    pub(crate) fn statistics(&self) -> HistoryResult<StoreStatistics> {
        self.read(|tx| {
            let rows = tx.query("statistics_totals", Vec::new())?;
            let row = rows
                .first()
                .filter(|_| rows.len() == 1)
                .ok_or(HistoryError::Integrity("Store totals"))?;
            let get = |index| -> HistoryResult<u64> {
                u64::try_from(cell::<i64>(row, index)?)
                    .map_err(|_| HistoryError::Integrity("negative Store size"))
            };
            let (pages, free, page_size) = (get(4)?, get(5)?, get(6)?);
            let (file, free) = (pages.checked_mul(page_size), free.checked_mul(page_size));
            let (file, free) = (
                file.ok_or(HistoryError::Capacity("Store size"))?,
                free.ok_or(HistoryError::Capacity("Store size"))?,
            );
            let stored = get(2)?;
            let overhead = file
                .checked_sub(free)
                .and_then(|used| used.checked_sub(stored))
                .ok_or(HistoryError::Integrity("Store allocation accounting"))?;
            Ok(StoreStatistics {
                logical: get(0)?,
                canonical: get(1)?,
                stored,
                metadata: get(3)?
                    .checked_add(overhead)
                    .ok_or(HistoryError::Capacity("metadata size"))?,
                file,
                free,
                // Init always scans its root, so no entries means not recorded.
                init: match get(7)? {
                    0 => None,
                    entries => Some(InitStatistics {
                        entries,
                        files: get(8)?,
                        bytes: get(9)?,
                        canonical: get(10)?,
                        stored: get(11)?,
                        metadata: get(12)?,
                        millis: get(13)?,
                    }),
                },
            })
        })
    }
    pub(crate) fn record_init(&self, totals: &InitStatistics) -> HistoryResult<()> {
        if totals.entries == 0 {
            return Err(HistoryError::InvalidInput("Init entries"));
        }
        let row = [
            totals.entries,
            totals.files,
            totals.bytes,
            totals.canonical,
            totals.stored,
            totals.metadata,
            totals.millis,
        ]
        .into_iter()
        .map(|n| i64::try_from(n).map(Param::I64))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| HistoryError::Capacity("Init totals"))?;
        let recorded = self.write(|tx| tx.execute("statistics_record_init", row))?;
        if recorded != 1 {
            return Err(HistoryError::Integrity("Init totals already recorded"));
        }
        Ok(())
    }
}

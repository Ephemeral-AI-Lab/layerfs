use layerfs_core::cdc::FastCdc;
use layerfs_core::ObjectId;
use std::fs::File;
use std::path::Path;

pub(super) struct CdcObservation {
    pub(super) bytes: u64,
    pub(super) references: u64,
    pub(super) sequence: String,
}

pub(super) fn scan_file(path: &Path) -> Result<CdcObservation, String> {
    let mut references = 0_u64;
    let mut sequence = blake3::Hasher::new();
    let counters = FastCdc::new()
        .scan(
            File::open(path).map_err(|error| error.to_string())?,
            |chunk| {
                let length = u32::try_from(chunk.len())
                    .map_err(|_| layerfs_core::CoreError::LengthOverflow)?;
                sequence.update(&length.to_be_bytes());
                sequence.update(ObjectId::for_bytes(chunk).as_bytes());
                references = references
                    .checked_add(1)
                    .ok_or(layerfs_core::CoreError::LengthOverflow)?;
                Ok(())
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(CdcObservation {
        bytes: counters.bytes_scanned,
        references,
        sequence: sequence.finalize().to_hex().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::scan_file;

    #[test]
    fn oracle_uses_frozen_core_profile_and_literal_raw_sequence() {
        let path = std::env::temp_dir().join(format!("layerfs-eval-cdc-{}", std::process::id()));
        std::fs::write(&path, b"fixture cdc oracle").unwrap();
        let first = scan_file(&path).unwrap();
        let second = scan_file(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(first.bytes, 18);
        assert_eq!(first.references, 1);
        assert_eq!(first.sequence, second.sequence);
        assert_eq!(
            first.sequence,
            "572cac4839e38c5ef1b829ffb09cb7f1d18dedc5e003f8ef8c0272131cdbcc11"
        );
    }
}

//! Fixed-record navigation for replacements resolved from the authenticated base.
use super::*;
use layerfs_content::{read_range, AuthenticatedObjects, ObjectId};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::io::Cursor;

pub(super) struct OriginRuns {
    file: SpoolFile,
    count: u64,
}

impl OriginRuns {
    pub(super) fn new() -> Result<Self, Failure> {
        Ok(Self {
            file: SpoolFile::create("origins")?,
            count: 0,
        })
    }

    pub(super) fn push(
        &mut self,
        start: u64,
        length: u64,
        kind: u64,
        source: u64,
    ) -> Result<(), Failure> {
        for value in [start, length, kind, source] {
            self.file.file.write_all(&value.to_be_bytes())?;
        }
        self.count = self.count.checked_add(1).ok_or(Code::Capacity)?;
        Ok(())
    }

    fn record(&self, index: u64) -> ContentResult<[u64; 4]> {
        if index >= self.count {
            return Err(ContentError::Io);
        }
        let mut file = &self.file.file;
        file.seek(SeekFrom::Start(
            index.checked_mul(32).ok_or(ContentError::LengthOverflow)?,
        ))
        .map_err(|_| ContentError::Io)?;
        let mut bytes = [0; 32];
        file.read_exact(&mut bytes).map_err(|_| ContentError::Io)?;
        Ok(std::array::from_fn(|index| {
            u64::from_be_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
        }))
    }

    fn at(&self, offset: u64) -> ContentResult<[u64; 4]> {
        let (mut low, mut high) = (0, self.count);
        while low < high {
            let mid = low + (high - low) / 2;
            if self.record(mid)?[0] <= offset {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let record = self.record(low.checked_sub(1).ok_or(ContentError::Io)?)?;
        if record[0]
            .checked_add(record[1])
            .is_none_or(|end| offset >= end)
        {
            return Err(ContentError::Io);
        }
        Ok(record)
    }

    pub(super) fn cleanup(&mut self) -> Result<(), Failure> {
        self.file.remove()
    }
}

pub(crate) struct ResolvedSource<'a, 't> {
    pub input: &'a FileInput,
    pub provider: &'a dyn AuthenticatedObjects,
    pub base: ObjectId,
    pub deadline: Instant,
    pub scope: &'a TimingScope<'t, Active>,
}

impl EditSource for ResolvedSource<'_, '_> {
    fn replacement_len(&self, index: usize) -> u64 {
        self.input.replacement_len(index)
    }

    fn read_at(&self, index: usize, offset: u64, out: &mut [u8]) -> ContentResult<usize> {
        let Some(origins) = &self.input.origins else {
            return self.input.read_at(index, offset, out);
        };
        let (edit, start) = self.input.record(index)?;
        if offset > edit.replacement_len() {
            return Err(ContentError::InvalidEdit {
                what: "replacement offset",
            });
        }
        let length = out.len().min((edit.replacement_len() - offset) as usize);
        let mut done = 0;
        while done < length {
            if Instant::now() >= self.deadline {
                return Err(ContentError::Io);
            }
            let at = start
                .checked_add(offset)
                .and_then(|at| at.checked_add(done as u64))
                .ok_or(ContentError::LengthOverflow)?;
            let [begin, run_length, kind, source] = origins.at(at)?;
            let skip = at - begin;
            let take = (length - done)
                .min(WINDOW_BYTES)
                .min((run_length - skip) as usize);
            let source = source
                .checked_add(skip)
                .ok_or(ContentError::LengthOverflow)?;
            let output = &mut out[done..done + take];
            match kind {
                0 => {
                    let end = source
                        .checked_add(take as u64)
                        .ok_or(ContentError::LengthOverflow)?;
                    let mut sink = Cursor::new(output);
                    read_range(
                        self.provider,
                        self.base,
                        source..end,
                        &mut sink,
                        self.scope.child("service.base_replacement"),
                    )?;
                    if sink.position() != take as u64 {
                        return Err(ContentError::Io);
                    }
                }
                1 => {
                    if self.input.read_bytes(source, output)? != take {
                        return Err(ContentError::Io);
                    }
                }
                _ => return Err(ContentError::Io),
            }
            done += take;
        }
        Ok(done)
    }
}

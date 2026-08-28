impl MountedWorkspace {
    fn normalize_spool(&mut self) -> Result<(), MountedError> {
        if self.spool.live == 0 {
            self.reset_spool_if_unused()
        } else {
            self.compact_spool_if_needed(0)
        }
    }

    fn normalize_spool_during_checkpoint(&mut self) -> Result<(), MountedError> {
        if self.spool.live == 0 {
            self.reset_spool_if_unused()
        } else if self.spool_needs_compaction(0)? {
            self.compact_spool_inner()
        } else {
            Ok(())
        }
    }

    fn reset_spool_if_unused(&mut self) -> Result<(), MountedError> {
        if self.spool.live == 0 && self.spool.reset()? {
            self.counters.spool_resets += 1;
        }
        Ok(())
    }

    fn compact_spool_if_needed(&mut self, additional_live: u64) -> Result<(), MountedError> {
        if self.spool_needs_compaction(additional_live)? {
            self.compact_spool()?;
        }
        Ok(())
    }

    fn spool_needs_compaction(&self, additional_live: u64) -> Result<bool, MountedError> {
        let projected_physical = self
            .spool
            .appended
            .checked_add(additional_live)
            .ok_or(MountedError::NoSpace)?;
        let projected_live = self
            .spool
            .live
            .checked_add(additional_live)
            .ok_or(MountedError::NoSpace)?;
        let steady_limit = projected_live
            .checked_mul(2)
            .and_then(|value| value.checked_add(SPOOL_COMPACTION_SLACK_BYTES))
            .ok_or(MountedError::NoSpace)?;
        Ok(projected_physical > SPOOL_QUOTA_BYTES || projected_physical > steady_limit)
    }

    fn compact_spool(&mut self) -> Result<(), MountedError> {
        let _reservation = self.budget.try_reserve(
            MAX_DIRTY_RANGES
                .checked_mul(std::mem::size_of::<SpoolRangeLocation>())
                .and_then(|bytes| bytes.checked_add(64 * 1024))
                .ok_or(MountedError::ResourceExhausted)?,
        )?;
        self.compact_spool_inner()
    }

    fn compact_spool_inner(&mut self) -> Result<(), MountedError> {
        if self.spool.appended == self.spool.live {
            return Ok(());
        }
        let mut locations = Vec::with_capacity(self.live_ranges);
        for (node, entry) in &self.nodes {
            if let NodeContent::File { ranges, .. } = &entry.content {
                for (start, range) in ranges {
                    locations.push(SpoolRangeLocation {
                        node: *node,
                        start: *start,
                        old_offset: range.spool_offset,
                        len: range.end - *start,
                    });
                }
            }
        }
        let live = locations.iter().try_fold(0_u64, |total, range| {
            total
                .checked_add(range.len)
                .ok_or(MountedError::Indeterminate)
        })?;
        if live != self.spool.live || locations.len() != self.live_ranges {
            return Err(MountedError::Indeterminate);
        }
        if live == 0 {
            return self.reset_spool_if_unused();
        }
        let compact = Spool::compaction_path(&self.spool.path);
        if compact.exists() {
            return Err(MountedError::Corrupt);
        }
        let result = (|| {
            let mut output = OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&compact)?;
            output.write_all(&self.spool.marker)?;
            let input = self.spool.file.as_mut().ok_or(MountedError::Corrupt)?;
            let mut buffer = [0_u8; 64 * 1024];
            let mut next = SPOOL_MARKER_BYTES;
            let mut offsets = Vec::with_capacity(locations.len());
            for location in &locations {
                input.seek(SeekFrom::Start(location.old_offset))?;
                offsets.push(next);
                let mut remaining = location.len;
                while remaining != 0 {
                    let count = buffer.len().min(remaining as usize);
                    input.read_exact(&mut buffer[..count])?;
                    output.write_all(&buffer[..count])?;
                    remaining -= count as u64;
                    next = next
                        .checked_add(count as u64)
                        .ok_or(MountedError::Indeterminate)?;
                }
            }
            output.sync_data()?;
            std::fs::rename(&compact, &self.spool.path)?;
            let old = self.spool.file.replace(output);
            drop(old);
            for (location, offset) in locations.iter().zip(offsets) {
                let entry = self
                    .nodes
                    .get_mut(&location.node)
                    .ok_or(MountedError::Indeterminate)?;
                let NodeContent::File { ranges, .. } = &mut entry.content else {
                    return Err(MountedError::Indeterminate);
                };
                ranges
                    .get_mut(&location.start)
                    .ok_or(MountedError::Indeterminate)?
                    .spool_offset = offset;
            }
            self.spool.appended = live;
            self.counters.spool_compactions += 1;
            Ok(())
        })();
        if result.is_err() && compact.exists() {
            let _ = std::fs::remove_file(compact);
        }
        result
    }
}

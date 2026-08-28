macro_rules! projection_replace_methods {
    () => {
            fn atomic_replace(
                &self,
                temp: Box<dyn OwnedTempHandle>,
                parent: &dyn DirectoryHandle,
                name: &[u8],
            ) -> Result<()> {
                let temp = temp
                    .into_any()
                    .downcast::<Temp>()
                    .map_err(|_| DriverError::Conflict)?;
                atomic_replace_temp(
                    temp,
                    dir(parent)?,
                    name,
                    None,
                    DirectoryDurability::ImmediateDirectoryDurability,
                    &self.facts,
                )
                .map(drop)
            }
            fn atomic_replace_with_directory_durability(
                &self,
                temp: Box<dyn OwnedTempHandle>,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                requested: DirectoryDurability,
            ) -> Result<DirectoryDurability> {
                let temp = temp
                    .into_any()
                    .downcast::<Temp>()
                    .map_err(|_| DriverError::Conflict)?;
                atomic_replace_temp(temp, dir(parent)?, name, None, requested, &self.facts)
            }
            fn atomic_replace_checked(
                &self,
                temp: Box<dyn OwnedTempHandle>,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: Option<&[u8]>,
            ) -> Result<()> {
                let temp = temp
                    .into_any()
                    .downcast::<Temp>()
                    .map_err(|_| DriverError::Conflict)?;
                atomic_replace_temp(
                    temp,
                    dir(parent)?,
                    name,
                    Some(expected),
                    DirectoryDurability::ImmediateDirectoryDurability,
                    &self.facts,
                )
                .map(drop)
            }
            fn create_symlink_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                target: &[u8],
                metadata: &NativeMetadata,
            ) -> Result<()> {
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_validate,
                    || super::metadata::preflight_symlink(metadata),
                )?;
                let parent_dir = dir(parent)?;
                super::ffi::symlink_at(&parent_dir.file, name, target)?;
                let entry = super::ffi::open_entry_at(&parent_dir.file, name)?;
                let mut apply_elapsed = 0;
                let applied: Result<()> = (|| {
                    write_metadata_values(&entry, metadata, &self.facts)?;
                    metadata_apply_step(&mut apply_elapsed, || {
                        super::ffi::set_symlink_mtime_at(
                            &parent_dir.file,
                            name,
                            metadata.mtime_seconds,
                            metadata.mtime_nanoseconds,
                        )
                        .map_err(Into::into)
                    })?;
                    Ok(())
                })();
                self.facts
                    .update(|facts| finish_call(&mut facts.metadata_apply, apply_elapsed, applied.is_ok()));
                applied?;
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_postinstall_verify,
                    || super::metadata::finish(&entry, metadata),
                )
            }
            fn atomic_replace_symlink(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: Option<&[u8]>,
                target: &[u8],
                metadata: &NativeMetadata,
            ) -> Result<()> {
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_validate,
                    || super::metadata::preflight_symlink(metadata),
                )?;
                let parent = &dir(parent)?.file;
                let prior = optional_token(parent, name)?;
                if prior.as_deref() != expected {
                    return Err(DriverError::Conflict);
                }
                let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
                for _ in 0..64 {
                    let temp_name = format!(
                        "symlink-{}-{}",
                        std::process::id(),
                        TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
                    )
                    .into_bytes();
                    match super::ffi::symlink_at(staging, &temp_name, target) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                        Err(error) => return Err(error.into()),
                    }
                    let requested = super::ffi::stable_token_at(staging, &temp_name)?;
                    let prepared = (|| -> Result<()> {
                        let entry = super::ffi::open_entry_at(staging, &temp_name)?;
                        let mut apply_elapsed = 0;
                        let applied: Result<()> = (|| {
                            write_metadata_values(&entry, metadata, &self.facts)?;
                            metadata_apply_step(&mut apply_elapsed, || {
                                super::ffi::set_symlink_mtime_at(
                                    staging,
                                    &temp_name,
                                    metadata.mtime_seconds,
                                    metadata.mtime_nanoseconds,
                                )
                                .map_err(Into::into)
                            })?;
                            Ok(())
                        })();
                        self.facts.update(|facts| {
                            finish_call(&mut facts.metadata_apply, apply_elapsed, applied.is_ok())
                        });
                        applied?;
                        observed_call(
                            &self.facts,
                            |facts| &mut facts.metadata_preinstall_verify,
                            || super::metadata::finish(&entry, metadata),
                        )
                    })();
                    if let Err(error) = prepared {
                        let cleanup_start = Instant::now();
                        let cleaned = super::ffi::unlink_if_identity_at(staging, &temp_name, &requested);
                        finish_cleanup(&self.facts, cleanup_start, cleaned.is_ok());
                        return Err(error);
                    }
                    let replace_start = Instant::now();
                    let replaced = match super::ffi::replace_at(staging, &temp_name, parent, name) {
                        Ok(()) => (|| {
                            if optional_token(parent, name)?.as_deref() != Some(requested.as_slice())
                                || optional_token(staging, &temp_name)?.is_some()
                            {
                                Err(DriverError::VisibilityAmbiguous)
                            } else {
                                Ok(())
                            }
                        })(),
                        Err(error) => reconcile_replace(parent, name, prior.clone(), &requested, error),
                    };
                    finish_replace(&self.facts, replace_start, prior.is_some(), &replaced);
                    if let Err(error) = replaced {
                        let cleanup_start = Instant::now();
                        let cleaned = super::ffi::unlink_if_identity_at(staging, &temp_name, &requested);
                        finish_cleanup(&self.facts, cleanup_start, cleaned.is_ok());
                        return Err(error);
                    }
                    let verified = observed_call(
                        &self.facts,
                        |facts| &mut facts.metadata_postinstall_verify,
                        || {
                            let entry = super::ffi::open_entry_at(parent, name)?;
                            super::metadata::verify(&entry, metadata)
                        },
                    );
                    if verified.is_err() {
                        return Err(DriverError::VisibilityAmbiguous);
                    }
                    let outcome = match sync_directory_file_io(
                        parent,
                        &self.facts,
                        DirectorySyncOwner::InstallParent,
                    ) {
                        Ok(()) => Ok(()),
                        Err(error) => reconcile_replace(parent, name, prior, &requested, error),
                    };
                    record_replace_durability_ambiguity(&self.facts, &outcome);
                    return outcome;
                }
                Err(DriverError::Conflict)
            }
    };
}

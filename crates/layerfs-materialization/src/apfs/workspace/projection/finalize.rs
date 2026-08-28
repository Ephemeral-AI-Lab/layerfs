macro_rules! projection_finalize_methods {
    () => {
            fn sync_directory(&self, directory: &dyn DirectoryHandle) -> Result<()> {
                let directory = dir(directory)?;
                let owner = match directory.role {
                    DirectoryRole::Root => DirectorySyncOwner::FinalRoot,
                    DirectoryRole::Tree => DirectorySyncOwner::DirtyTree,
                };
                sync_directory_file(&directory.file, &self.facts, owner)
            }
            fn set_root_metadata(&self, metadata: &NativeMetadata) -> Result<()> {
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_validate,
                    || super::metadata::preflight(&self.root_dir, metadata),
                )?;
                let mut apply_elapsed = 0;
                let applied: Result<()> = (|| {
                    metadata_apply_step(&mut apply_elapsed, || {
                        self.root_dir
                            .set_permissions(fs::Permissions::from_mode(metadata.mode))
                            .map_err(Into::into)
                    })?;
                    write_metadata_values(&self.root_dir, metadata, &self.facts)?;
                    let modified = modified_time(metadata)?;
                    metadata_apply_step(&mut apply_elapsed, || {
                        self.root_dir
                            .set_times(FileTimes::new().set_modified(modified))
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
                    || super::metadata::finish(&self.root_dir, metadata),
                )
            }
            fn discard_owned_root(self: Box<Self>) -> Result<()> {
                if !self.owned_root {
                    return Err(DriverError::Conflict);
                }
                self.remove_owned_root(&self.root_identity)
            }
            fn remove_owned_root(&self, expected_identity: &[u8]) -> Result<()> {
                if super::ffi::file_stable_token(&self.root_dir)? != expected_identity {
                    return Err(DriverError::Conflict);
                }
                for _ in 0..64 {
                    let tombstone = format!(
                        ".layerfs-owned-tombstone-{}-{}",
                        std::process::id(),
                        TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
                    )
                    .into_bytes();
                    let start = Instant::now();
                    let removed = super::ffi::detach_and_remove_owned_tree(
                        &self.root_dir,
                        &self.root_parent,
                        &self.root_name,
                        &tombstone,
                        expected_identity,
                    );
                    finish_cleanup(&self.facts, start, removed.is_ok());
                    match removed {
                        Ok(()) => return Ok(()),
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) if error.raw_os_error() == Some(libc::ESTALE) => {
                            return Err(DriverError::VisibilityAmbiguous)
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(DriverError::Conflict)
            }
    };
}

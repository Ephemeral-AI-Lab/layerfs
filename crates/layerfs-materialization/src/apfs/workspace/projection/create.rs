macro_rules! projection_create_methods {
    () => {
            fn create_directory_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
            ) -> Result<Box<dyn DirectoryHandle>> {
                let parent = dir(parent)?;
                super::ffi::mkdir_at(&parent.file, name)?;
                match super::ffi::open_directory_at(&parent.file, name) {
                    Ok(file) => Ok(Box::new(Dir {
                        file,
                        role: DirectoryRole::Tree,
                    })),
                    Err(_) => Err(DriverError::VisibilityAmbiguous),
                }
            }
            fn create_temp_at(&self, _parent: &dyn DirectoryHandle) -> Result<Box<dyn OwnedTempHandle>> {
                for _ in 0..64 {
                    let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
                    let name = format!("temp-{}-{serial}", std::process::id()).into_bytes();
                    let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
                    let start = Instant::now();
                    let created = super::ffi::create_regular_at(staging, &name);
                    let elapsed = elapsed_ns(start);
                    self.facts
                        .update(|facts| finish_call(&mut facts.temp_create, elapsed, created.is_ok()));
                    match created {
                        Ok(file) => {
                            let identity = super::ffi::file_stable_token(&file)?;
                            return Ok(Box::new(Temp {
                                facts: self.facts.clone(),
                                file,
                                staging: staging.try_clone()?,
                                name,
                                identity,
                                expected_metadata: Mutex::new(None),
                                deferred_flags: 0,
                            }));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(DriverError::Conflict)
            }
            fn clone_temp_from_regular(
                &self,
                source: &dyn RegularFileHandle,
            ) -> Result<Box<dyn OwnedTempHandle>> {
                let source = source
                    .as_any()
                    .downcast_ref::<Regular>()
                    .ok_or(DriverError::Conflict)?;
                if source.0.metadata()?.nlink() != 1 {
                    return Err(DriverError::Unsupported);
                }
                let metadata = super::metadata::read(&source.0)?;
                let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
                for _ in 0..64 {
                    let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
                    let name = format!("clone-{}-{serial}", std::process::id()).into_bytes();
                    let start = Instant::now();
                    let created = super::ffi::clone_file_at(&source.0, staging, &name);
                    let elapsed = elapsed_ns(start);
                    self.facts
                        .update(|facts| finish_call(&mut facts.temp_create, elapsed, created.is_ok()));
                    match created {
                        Ok(file) => {
                            let identity = super::ffi::file_stable_token(&file)?;
                            super::ffi::set_flags_file(&file, 0)?;
                            return Ok(Box::new(Temp {
                                facts: self.facts.clone(),
                                file,
                                staging: staging.try_clone()?,
                                name,
                                identity,
                                expected_metadata: Mutex::new(None),
                                deferred_flags: metadata.bsd_flags,
                            }));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) if matches!(error.raw_os_error(), Some(code) if code == libc::ENOTSUP || code == libc::EXDEV || code == libc::EOPNOTSUPP) => {
                            return Err(DriverError::Unsupported)
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(DriverError::Conflict)
            }
            fn read_temp_metadata(&self, temp: &dyn OwnedTempHandle) -> Result<NativeMetadata> {
                let temp = temp
                    .as_any()
                    .downcast_ref::<Temp>()
                    .ok_or(DriverError::Conflict)?;
                let mut metadata = super::metadata::read(&temp.file)?;
                metadata.bsd_flags = temp.deferred_flags;
                Ok(metadata)
            }
            fn set_temp_metadata(
                &self,
                temp: &mut dyn OwnedTempHandle,
                metadata: &NativeMetadata,
            ) -> Result<()> {
                let temp = temp
                    .as_any()
                    .downcast_ref::<Temp>()
                    .ok_or(DriverError::Conflict)?;
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_validate,
                    || super::metadata::preflight(&temp.file, metadata),
                )?;
                let mut apply_elapsed = 0;
                let applied: Result<()> = (|| {
                    metadata_apply_step(&mut apply_elapsed, || {
                        temp.file
                            .set_permissions(fs::Permissions::from_mode(metadata.mode))
                            .map_err(Into::into)
                    })?;
                    write_metadata_values(&temp.file, metadata, &self.facts)?;
                    let modified = modified_time(metadata)?;
                    metadata_apply_step(&mut apply_elapsed, || {
                        temp.file
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
                    |facts| &mut facts.metadata_preinstall_verify,
                    || super::metadata::verify_before_install(&temp.file, metadata),
                )?;
                *temp
                    .expected_metadata
                    .lock()
                    .map_err(|_| DriverError::Conflict)? = Some(metadata.clone());
                Ok(())
            }
            fn set_entry_metadata(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: &[u8],
                metadata: &NativeMetadata,
            ) -> Result<()> {
                let parent = &dir(parent)?.file;
                if super::ffi::stable_token_at(parent, name)? != expected {
                    return Err(DriverError::Conflict);
                }
                let entry = super::ffi::open_entry_at(parent, name)?;
                if super::ffi::file_stable_token(&entry)? != expected {
                    return Err(DriverError::Conflict);
                }
                let native = entry.metadata()?;
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_validate,
                    || super::metadata::preflight(&entry, metadata),
                )?;
                if native.file_type().is_symlink() {
                    let mut apply_elapsed = 0;
                    let applied: Result<()> = (|| {
                        write_metadata_values(&entry, metadata, &self.facts)?;
                        metadata_apply_step(&mut apply_elapsed, || {
                            super::ffi::set_symlink_mtime_at(
                                parent,
                                name,
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
                    return observed_call(
                        &self.facts,
                        |facts| &mut facts.metadata_postinstall_verify,
                        || super::metadata::finish(&entry, metadata),
                    );
                }
                let mut apply_elapsed = 0;
                let applied: Result<()> = (|| {
                    metadata_apply_step(&mut apply_elapsed, || {
                        entry
                            .set_permissions(fs::Permissions::from_mode(metadata.mode))
                            .map_err(Into::into)
                    })?;
                    write_metadata_values(&entry, metadata, &self.facts)?;
                    let modified = modified_time(metadata)?;
                    metadata_apply_step(&mut apply_elapsed, || {
                        entry
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
                    || super::metadata::finish(&entry, metadata),
                )
            }
    };
}

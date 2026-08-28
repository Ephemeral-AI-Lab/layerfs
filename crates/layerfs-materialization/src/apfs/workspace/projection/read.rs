macro_rules! projection_read_methods {
    () => {
            fn projection_facts(&self) -> ProjectionFacts {
                self.facts.snapshot()
            }

            fn root_directory(&self) -> Result<Box<dyn DirectoryHandle>> {
                Ok(Box::new(Dir {
                    file: self.root_dir.try_clone()?,
                    role: DirectoryRole::Root,
                }))
            }

            fn enumerate_at<'a>(
                &'a self,
                parent: &'a dyn DirectoryHandle,
            ) -> Result<Box<dyn Iterator<Item = Result<NativeEntry>> + 'a>> {
                let parent = dir(parent)?;
                Ok(Box::new(super::ffi::directory_entries(&parent.file)?.map(
                    |entry| {
                        let (name, kind, link_count, token, stable) = entry?;
                        Ok(NativeEntry {
                            name,
                            kind,
                            token,
                            hard_link_key: (kind == NativeKind::RegularFile).then_some(stable),
                            link_count,
                        })
                    },
                )))
            }

            fn open_directory_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: Option<&[u8]>,
            ) -> Result<Box<dyn DirectoryHandle>> {
                let parent = dir(parent)?;
                let file = super::ffi::open_directory_at(&parent.file, name)?;
                validate_expected(&file, expected)?;
                Ok(Box::new(Dir {
                    file,
                    role: DirectoryRole::Tree,
                }))
            }
            fn duplicate_directory(
                &self,
                directory: &dyn DirectoryHandle,
            ) -> Result<Box<dyn DirectoryHandle>> {
                let directory = dir(directory)?;
                Ok(Box::new(Dir {
                    file: directory.file.try_clone()?,
                    role: directory.role,
                }))
            }
            fn directory_token(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
                Ok(super::ffi::file_token(&dir(directory)?.file)?)
            }
            fn directory_identity(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
                Ok(super::ffi::file_stable_token(&dir(directory)?.file)?)
            }
            fn revalidate_root_binding(&self) -> Result<()> {
                let start = Instant::now();
                let result = (|| {
                    if super::ffi::stable_token_at(&self.root_parent, &self.root_name)?
                        != super::ffi::file_stable_token(&self.root_dir)?
                    {
                        return Err(DriverError::Conflict);
                    }
                    Ok(())
                })();
                let elapsed = elapsed_ns(start);
                self.facts.update(|facts| {
                    finish_call(&mut facts.root_binding_revalidate, elapsed, result.is_ok());
                    finish_call(&mut facts.authority_completion, elapsed, result.is_ok());
                });
                result
            }
            fn begin_name_preflight(&self) -> Result<Box<dyn NamePreflight>> {
                let started = Instant::now();
                let result = (|| {
                    let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
                    let name = format!(
                        "preflight-{}-{}",
                        std::process::id(),
                        TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
                    )
                    .into_bytes();
                    super::ffi::mkdir_at(staging, &name)?;
                    let directory = super::ffi::open_directory_at(staging, &name)?;
                    let identity = super::ffi::file_stable_token(&directory)?;
                    Ok::<_, DriverError>(Box::new(Preflight {
                        facts: self.facts.clone(),
                        wall_ns: elapsed_ns(started),
                        observed: false,
                        directory,
                        staging: staging.try_clone()?,
                        name,
                        identity,
                        active: true,
                    }) as Box<dyn NamePreflight>)
                })();
                if result.is_err() {
                    let elapsed = elapsed_ns(started);
                    self.facts
                        .update(|facts| finish_call(&mut facts.name_preflight, elapsed, false));
                }
                result
            }
            fn open_regular_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: Option<&[u8]>,
            ) -> Result<Box<dyn RegularFileHandle>> {
                let file = super::ffi::open_regular_at(&dir(parent)?.file, name, self.managed)?;
                validate_expected(&file, expected)?;
                Ok(Box::new(Regular(file, self.facts.clone())))
            }
            fn open_regular_read_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: Option<&[u8]>,
            ) -> Result<Box<dyn RegularFileHandle>> {
                let file = super::ffi::open_regular_at(&dir(parent)?.file, name, false)?;
                validate_expected(&file, expected)?;
                Ok(Box::new(Regular(file, self.facts.clone())))
            }
            fn set_regular_len(&self, file: &mut dyn RegularFileHandle, len: u64) -> Result<()> {
                file.as_any()
                    .downcast_ref::<Regular>()
                    .ok_or(DriverError::Conflict)?
                    .0
                    .set_len(len)?;
                Ok(())
            }
            fn sync_regular(&self, file: &mut dyn RegularFileHandle) -> Result<()> {
                let file = file
                    .as_any()
                    .downcast_ref::<Regular>()
                    .ok_or(DriverError::Conflict)?;
                let start = Instant::now();
                let result = file.0.sync_all();
                let elapsed = elapsed_ns(start);
                self.facts
                    .update(|facts| finish_sync(&mut facts.regular_file_sync, elapsed, result.is_ok()));
                result.map_err(Into::into)
            }
            fn read_link_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: Option<&[u8]>,
            ) -> Result<Vec<u8>> {
                let parent = &dir(parent)?.file;
                validate_entry_expected(parent, name, expected)?;
                let target = super::ffi::read_link_at(parent, name)?;
                validate_entry_expected(parent, name, expected)?;
                Ok(target)
            }
            fn read_metadata_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: Option<&[u8]>,
            ) -> Result<NativeMetadata> {
                let entry = super::ffi::open_entry_at(&dir(parent)?.file, name)?;
                validate_expected(&entry, expected)?;
                let metadata = super::metadata::read(&entry)?;
                validate_expected(&entry, expected)?;
                Ok(metadata)
            }
            fn token_at(&self, parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>> {
                Ok(super::ffi::token_at(&dir(parent)?.file, name)?)
            }
            fn identity_at(&self, parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>> {
                Ok(super::ffi::stable_token_at(&dir(parent)?.file, name)?)
            }
            fn read_root_metadata(&self) -> Result<NativeMetadata> {
                super::metadata::read(&self.root_dir)
            }
            fn read_directory_metadata(&self, directory: &dyn DirectoryHandle) -> Result<NativeMetadata> {
                super::metadata::read(&dir(directory)?.file)
            }
    };
}

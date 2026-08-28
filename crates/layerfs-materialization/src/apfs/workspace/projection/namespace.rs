macro_rules! projection_namespace_methods {
    () => {
            fn create_hard_link_at(
                &self,
                source_parent: &dyn DirectoryHandle,
                source: &[u8],
                source_expected: &[u8],
                target_parent: &dyn DirectoryHandle,
                target: &[u8],
            ) -> Result<()> {
                let source_parent = &dir(source_parent)?.file;
                if super::ffi::stable_token_at(source_parent, source)? != source_expected {
                    return Err(DriverError::Conflict);
                }
                super::ffi::hard_link_at(source_parent, source, &dir(target_parent)?.file, target)?;
                if super::ffi::stable_token_at(source_parent, source)
                    .map(|actual| actual != source_expected)
                    .unwrap_or(true)
                {
                    return Err(DriverError::VisibilityAmbiguous);
                }
                Ok(())
            }
            fn finish_hard_link_at(
                &self,
                source_parent: &dyn DirectoryHandle,
                source: &[u8],
                source_expected: &[u8],
                metadata: &NativeMetadata,
            ) -> Result<()> {
                let source_parent = &dir(source_parent)?.file;
                if super::ffi::stable_token_at(source_parent, source)? != source_expected {
                    return Err(DriverError::Conflict);
                }
                let entry = super::ffi::open_entry_at(source_parent, source)?;
                if super::ffi::file_stable_token(&entry)? != source_expected {
                    return Err(DriverError::Conflict);
                }
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_validate,
                    || super::metadata::preflight(&entry, metadata),
                )?;
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_postinstall_verify,
                    || super::metadata::finish(&entry, metadata),
                )?;
                sync_file(&entry, &self.facts, FileSyncOwner::PostHardLink)
            }
            fn rename_at(
                &self,
                source_parent: &dyn DirectoryHandle,
                source: &[u8],
                target_parent: &dyn DirectoryHandle,
                target: &[u8],
            ) -> Result<()> {
                let source_parent = &dir(source_parent)?.file;
                let target_parent = &dir(target_parent)?.file;
                let requested = super::ffi::stable_token_at(source_parent, source)?;
                if let Err(error) = super::ffi::rename_at(source_parent, source, target_parent, target) {
                    return reconcile_rename(
                        source_parent,
                        source,
                        target_parent,
                        target,
                        &requested,
                        error,
                    );
                }
                let sync = if super::ffi::file_stable_token(source_parent)?
                    == super::ffi::file_stable_token(target_parent)?
                {
                    sync_directory_file_io(
                        source_parent,
                        &self.facts,
                        DirectorySyncOwner::InstallParent,
                    )
                } else {
                    sync_directory_file_io(
                        source_parent,
                        &self.facts,
                        DirectorySyncOwner::InstallParent,
                    )
                    .and_then(|_| {
                        sync_directory_file_io(
                            target_parent,
                            &self.facts,
                            DirectorySyncOwner::InstallParent,
                        )
                    })
                };
                if let Err(error) = sync {
                    return reconcile_rename(
                        source_parent,
                        source,
                        target_parent,
                        target,
                        &requested,
                        error,
                    );
                }
                Ok(())
            }
            fn unlink_regular_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: &[u8],
            ) -> Result<()> {
                remove_entry(&dir(parent)?.file, name, expected, false, &self.facts)
            }
            fn unlink_symlink_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: &[u8],
            ) -> Result<()> {
                remove_entry(&dir(parent)?.file, name, expected, false, &self.facts)
            }
            fn remove_directory_at(
                &self,
                parent: &dyn DirectoryHandle,
                name: &[u8],
                expected: &[u8],
            ) -> Result<()> {
                remove_entry(&dir(parent)?.file, name, expected, true, &self.facts)
            }
    };
}

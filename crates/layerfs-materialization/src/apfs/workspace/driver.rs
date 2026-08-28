use super::*;

impl ProjectionDriver for AppleDriver {
    fn projection_facts(&self) -> ProjectionFacts {
        self.facts.snapshot()
    }

    fn open_workspace(
        &self,
        path: &Path,
        policy: WorkspacePolicy,
        store_id: [u8; 32],
    ) -> Result<Box<dyn ProjectionWorkspace>> {
        let setup_start = Instant::now();
        let result = (|| {
            let root_start = Instant::now();
            let root_result = (|| {
                let parent_path = path.parent().unwrap_or_else(|| Path::new("."));
                let root_parent = super::ffi::open_directory_path_nofollow(parent_path)?;
                let root_name = path
                    .file_name()
                    .ok_or(DriverError::Conflict)?
                    .as_bytes()
                    .to_vec();
                let mut cleanup = SetupCleanup {
                    facts: &self.facts,
                    root_parent: Some(root_parent),
                    root: SetupDirectory {
                        name: root_name,
                        file: None,
                        identity: None,
                        owned: false,
                    },
                    staging: None,
                    active: true,
                };
                let parent = cleanup
                    .root_parent
                    .as_ref()
                    .expect("root parent is present");
                let root_dir = if policy == WorkspacePolicy::ManagedCreateOwned {
                    match super::ffi::mkdir_at(parent, &cleanup.root.name) {
                        Ok(()) => {
                            cleanup.root.owned = true;
                            cleanup.root.identity =
                                Some(super::ffi::stable_token_at(parent, &cleanup.root.name)?);
                            #[cfg(test)]
                            setup_fault(SetupFault::ManagedRootCreated)?;
                            super::ffi::open_directory_at(parent, &cleanup.root.name)?
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                            return Err(DriverError::Conflict)
                        }
                        Err(error) => return Err(error.into()),
                    }
                } else {
                    match super::ffi::open_directory_at(parent, &cleanup.root.name) {
                        Ok(file) => file,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::NotFound
                                && policy == WorkspacePolicy::ExternalCooperative =>
                        {
                            super::ffi::mkdir_at(parent, &cleanup.root.name)?;
                            super::ffi::open_directory_at(parent, &cleanup.root.name)?
                        }
                        Err(error) => return Err(error.into()),
                    }
                };
                cleanup.root.file = Some(root_dir);
                #[cfg(test)]
                setup_fault(SetupFault::RootOpened)?;
                let root_identity = super::ffi::file_stable_token(
                    cleanup.root.file.as_ref().expect("root directory is open"),
                )?;
                if cleanup
                    .root
                    .identity
                    .as_deref()
                    .is_some_and(|expected| expected != root_identity)
                {
                    return Err(std::io::Error::from_raw_os_error(libc::ESTALE).into());
                }
                cleanup.root.identity = Some(root_identity);
                #[cfg(test)]
                setup_fault(SetupFault::RootIdentified)?;
                Ok::<_, DriverError>(cleanup)
            })();
            let root_elapsed = elapsed_ns(root_start);
            self.facts.update(|facts| {
                finish_call(
                    &mut facts.workspace_root_create_open,
                    root_elapsed,
                    root_result.is_ok(),
                )
            });
            let mut setup_cleanup = root_result?;
            let staging_start = Instant::now();
            let staging_result = (|| {
                for _ in 0..64 {
                    let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
                    let name =
                        format!(".layerfs-staging-{}-{serial}", std::process::id()).into_bytes();
                    match super::ffi::mkdir_at(
                        setup_cleanup
                            .root_parent
                            .as_ref()
                            .expect("root parent is present"),
                        &name,
                    ) {
                        Ok(()) => {
                            setup_cleanup.staging = Some(SetupDirectory {
                                name,
                                file: None,
                                identity: None,
                                owned: true,
                            });
                            let identity = super::ffi::stable_token_at(
                                setup_cleanup
                                    .root_parent
                                    .as_ref()
                                    .expect("root parent is present"),
                                &setup_cleanup
                                    .staging
                                    .as_ref()
                                    .expect("staging ownership was recorded")
                                    .name,
                            )?;
                            setup_cleanup
                                .staging
                                .as_mut()
                                .expect("staging ownership was recorded")
                                .identity = Some(identity);
                            #[cfg(test)]
                            setup_fault(SetupFault::StagingCreated)?;
                            let staging = setup_cleanup
                                .staging
                                .as_mut()
                                .expect("staging ownership was recorded");
                            staging.file = Some(super::ffi::open_directory_at(
                                setup_cleanup
                                    .root_parent
                                    .as_ref()
                                    .expect("root parent is present"),
                                &staging.name,
                            )?);
                            #[cfg(test)]
                            setup_fault(SetupFault::StagingOpened)?;
                            let actual = super::ffi::file_stable_token(
                                staging.file.as_ref().expect("staging directory is open"),
                            )?;
                            if staging.identity.as_deref() != Some(actual.as_slice()) {
                                return Err(std::io::Error::from_raw_os_error(libc::ESTALE).into());
                            }
                            #[cfg(test)]
                            setup_fault(SetupFault::StagingIdentified)?;
                            return Ok::<_, DriverError>(());
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(DriverError::Conflict)
            })();
            let staging_elapsed = elapsed_ns(staging_start);
            self.facts.update(|facts| {
                finish_call(
                    &mut facts.staging_create_open,
                    staging_elapsed,
                    staging_result.is_ok(),
                )
            });
            staging_result?;
            let staging_dir = setup_cleanup
                .staging
                .as_ref()
                .and_then(|staging| staging.file.as_ref())
                .expect("successful staging setup has an open directory");
            staging_dir.set_permissions(fs::Permissions::from_mode(0o700))?;
            let marker_start = Instant::now();
            let marker_result = super::ffi::create_regular_at(staging_dir, RECOVERY_MARKER);
            let marker_elapsed = elapsed_ns(marker_start);
            self.facts.update(|facts| {
                finish_call(
                    &mut facts.recovery_marker_create,
                    marker_elapsed,
                    marker_result.is_ok(),
                )
            });
            let mut recovery_marker = marker_result?;
            #[cfg(test)]
            setup_fault(SetupFault::MarkerCreated)?;
            recovery_marker.set_permissions(fs::Permissions::from_mode(0o600))?;
            MarkerWriter {
                file: &mut recovery_marker,
                facts: &self.facts,
            }
            .write_all(&encode_recovery_record(
                store_id,
                policy == WorkspacePolicy::ManagedCreateOwned,
                &setup_cleanup.root.name,
                setup_cleanup
                    .root
                    .identity
                    .as_ref()
                    .expect("successful root setup has an identity"),
            ))?;
            #[cfg(test)]
            setup_fault(SetupFault::MarkerWritten)?;
            sync_file(&recovery_marker, &self.facts, FileSyncOwner::RecoveryMarker)?;
            #[cfg(test)]
            setup_fault(SetupFault::MarkerSynced)?;
            if !super::ffi::try_lock_exclusive(&recovery_marker)? {
                return Err(DriverError::Conflict);
            }
            #[cfg(test)]
            setup_fault(SetupFault::MarkerLocked)?;
            sync_directory_file(staging_dir, &self.facts, DirectorySyncOwner::Staging)?;
            #[cfg(test)]
            setup_fault(SetupFault::StagingSynced)?;
            sync_directory_file(
                setup_cleanup
                    .root_parent
                    .as_ref()
                    .expect("root parent is present"),
                &self.facts,
                DirectorySyncOwner::RootParent,
            )?;
            #[cfg(test)]
            setup_fault(SetupFault::RootParentSynced)?;
            let workspace_root_parent = setup_cleanup
                .root_parent
                .as_ref()
                .expect("root parent is present")
                .try_clone()?;
            let root_dir = setup_cleanup
                .root
                .file
                .take()
                .expect("successful root setup has an open directory");
            let root_name = std::mem::take(&mut setup_cleanup.root.name);
            let root_identity = setup_cleanup
                .root
                .identity
                .take()
                .expect("successful root setup has an identity");
            let mut staging = setup_cleanup
                .staging
                .take()
                .expect("successful setup has staging ownership");
            let staging_dir = staging
                .file
                .take()
                .expect("successful staging setup has an open directory");
            let staging_name = staging.name;
            let staging_identity = staging
                .identity
                .take()
                .expect("successful staging setup has an identity");
            let root_parent = setup_cleanup
                .root_parent
                .take()
                .expect("root parent is present");
            setup_cleanup.active = false;
            drop(setup_cleanup);
            Ok::<_, DriverError>(Box::new(Workspace {
                facts: self.facts.clone(),
                root_dir,
                root_parent: workspace_root_parent,
                root_name,
                root_identity,
                staging_dir: Some(staging_dir),
                staging_parent: root_parent,
                staging_name,
                staging_identity,
                _recovery_marker: recovery_marker,
                managed: policy != WorkspacePolicy::ExternalCooperative,
                owned_root: policy == WorkspacePolicy::ManagedCreateOwned,
            }) as Box<dyn ProjectionWorkspace>)
        })();
        let setup_elapsed = elapsed_ns(setup_start);
        self.facts
            .update(|facts| finish_call(&mut facts.workspace_setup, setup_elapsed, result.is_ok()));
        result
    }

    fn recover_owned_workspaces(&self, parent: &Path, store_id: [u8; 32]) -> Result<()> {
        recover_owned_workspaces(parent, store_id, &self.facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_policy_separates_private_write_and_owned_cleanup() {
        let parent = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "layerfs-workspace-policy-{}-{}",
                std::process::id(),
                TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir(&parent).unwrap();
        let driver = AppleDriver::default();

        let private = parent.join("private");
        fs::create_dir(&private).unwrap();
        fs::write(private.join("file"), b"before").unwrap();
        let workspace = driver
            .open_workspace(&private, WorkspacePolicy::ManagedPrivate, [0x31; 32])
            .unwrap();
        let root = workspace.root_directory().unwrap();
        workspace
            .open_regular_at(root.as_ref(), b"file", None)
            .unwrap()
            .write_all(b"!")
            .unwrap();
        drop(workspace);
        assert!(private.exists());

        let owned = parent.join("owned");
        let workspace = driver
            .open_workspace(&owned, WorkspacePolicy::ManagedCreateOwned, [0x32; 32])
            .unwrap();
        assert!(owned.exists());
        workspace.discard_owned_root().unwrap();
        assert!(!owned.exists());

        let external = parent.join("external");
        let workspace = driver
            .open_workspace(&external, WorkspacePolicy::ExternalCooperative, [0x33; 32])
            .unwrap();
        assert!(matches!(
            workspace.discard_owned_root(),
            Err(DriverError::Conflict)
        ));
        assert!(external.exists());

        fs::remove_dir_all(parent).unwrap();
    }
}

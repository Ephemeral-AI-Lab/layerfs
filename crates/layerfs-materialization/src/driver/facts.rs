//! Aggregate projection telemetry.

use super::{
    ProjectionCallFacts, ProjectionCleanupFacts, ProjectionReplaceFacts, ProjectionSyncFacts,
    ProjectionWriteFacts,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionFacts {
    pub workspace_setup: ProjectionCallFacts,
    pub workspace_root_create_open: ProjectionCallFacts,
    pub staging_create_open: ProjectionCallFacts,
    pub recovery_marker_create: ProjectionCallFacts,
    pub name_preflight: ProjectionCallFacts,
    pub temp_create: ProjectionCallFacts,
    pub workspace_marker_write: ProjectionWriteFacts,
    pub content_write: ProjectionWriteFacts,
    pub metadata_value_write: ProjectionWriteFacts,
    /// Inclusive report-only sum of marker, content, and metadata writes.
    pub aggregate_native_write: ProjectionWriteFacts,
    pub content_flush: ProjectionCallFacts,
    pub metadata_validate: ProjectionCallFacts,
    pub metadata_apply: ProjectionCallFacts,
    pub metadata_preinstall_verify: ProjectionCallFacts,
    pub metadata_postinstall_verify: ProjectionCallFacts,
    pub root_binding_revalidate: ProjectionCallFacts,
    pub regular_file_sync: ProjectionSyncFacts,
    pub directory_sync: ProjectionSyncFacts,
    pub recovery_marker_file_sync: ProjectionSyncFacts,
    pub content_temp_file_sync: ProjectionSyncFacts,
    pub post_hardlink_file_sync: ProjectionSyncFacts,
    pub staging_directory_sync: ProjectionSyncFacts,
    pub root_parent_directory_sync: ProjectionSyncFacts,
    pub install_parent_directory_sync: ProjectionSyncFacts,
    pub dirty_tree_directory_sync: ProjectionSyncFacts,
    pub final_root_directory_sync: ProjectionSyncFacts,
    pub replace: ProjectionReplaceFacts,
    pub authority_completion: ProjectionCallFacts,
    pub cleanup: ProjectionCleanupFacts,
}

impl ProjectionFacts {
    pub const fn available() -> Self {
        Self {
            workspace_setup: ProjectionCallFacts::available(),
            workspace_root_create_open: ProjectionCallFacts::available(),
            staging_create_open: ProjectionCallFacts::available(),
            recovery_marker_create: ProjectionCallFacts::available(),
            name_preflight: ProjectionCallFacts::available(),
            temp_create: ProjectionCallFacts::available(),
            workspace_marker_write: ProjectionWriteFacts::available(),
            content_write: ProjectionWriteFacts::available(),
            metadata_value_write: ProjectionWriteFacts::available(),
            aggregate_native_write: ProjectionWriteFacts::available(),
            content_flush: ProjectionCallFacts::available(),
            metadata_validate: ProjectionCallFacts::available(),
            metadata_apply: ProjectionCallFacts::available(),
            metadata_preinstall_verify: ProjectionCallFacts::available(),
            metadata_postinstall_verify: ProjectionCallFacts::available(),
            root_binding_revalidate: ProjectionCallFacts::available(),
            regular_file_sync: ProjectionSyncFacts::available(),
            directory_sync: ProjectionSyncFacts::available(),
            recovery_marker_file_sync: ProjectionSyncFacts::available(),
            content_temp_file_sync: ProjectionSyncFacts::available(),
            post_hardlink_file_sync: ProjectionSyncFacts::available(),
            staging_directory_sync: ProjectionSyncFacts::available(),
            root_parent_directory_sync: ProjectionSyncFacts::available(),
            install_parent_directory_sync: ProjectionSyncFacts::available(),
            dirty_tree_directory_sync: ProjectionSyncFacts::available(),
            final_root_directory_sync: ProjectionSyncFacts::available(),
            replace: ProjectionReplaceFacts::available(),
            authority_completion: ProjectionCallFacts::available(),
            cleanup: ProjectionCleanupFacts::available(),
        }
    }

    pub fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            workspace_setup: self.workspace_setup.checked_delta(before.workspace_setup)?,
            workspace_root_create_open: self
                .workspace_root_create_open
                .checked_delta(before.workspace_root_create_open)?,
            staging_create_open: self
                .staging_create_open
                .checked_delta(before.staging_create_open)?,
            recovery_marker_create: self
                .recovery_marker_create
                .checked_delta(before.recovery_marker_create)?,
            name_preflight: self.name_preflight.checked_delta(before.name_preflight)?,
            temp_create: self.temp_create.checked_delta(before.temp_create)?,
            workspace_marker_write: self
                .workspace_marker_write
                .checked_delta(before.workspace_marker_write)?,
            content_write: self.content_write.checked_delta(before.content_write)?,
            metadata_value_write: self
                .metadata_value_write
                .checked_delta(before.metadata_value_write)?,
            aggregate_native_write: self
                .aggregate_native_write
                .checked_delta(before.aggregate_native_write)?,
            content_flush: self.content_flush.checked_delta(before.content_flush)?,
            metadata_validate: self
                .metadata_validate
                .checked_delta(before.metadata_validate)?,
            metadata_apply: self.metadata_apply.checked_delta(before.metadata_apply)?,
            metadata_preinstall_verify: self
                .metadata_preinstall_verify
                .checked_delta(before.metadata_preinstall_verify)?,
            metadata_postinstall_verify: self
                .metadata_postinstall_verify
                .checked_delta(before.metadata_postinstall_verify)?,
            root_binding_revalidate: self
                .root_binding_revalidate
                .checked_delta(before.root_binding_revalidate)?,
            regular_file_sync: self
                .regular_file_sync
                .checked_delta(before.regular_file_sync)?,
            directory_sync: self.directory_sync.checked_delta(before.directory_sync)?,
            recovery_marker_file_sync: self
                .recovery_marker_file_sync
                .checked_delta(before.recovery_marker_file_sync)?,
            content_temp_file_sync: self
                .content_temp_file_sync
                .checked_delta(before.content_temp_file_sync)?,
            post_hardlink_file_sync: self
                .post_hardlink_file_sync
                .checked_delta(before.post_hardlink_file_sync)?,
            staging_directory_sync: self
                .staging_directory_sync
                .checked_delta(before.staging_directory_sync)?,
            root_parent_directory_sync: self
                .root_parent_directory_sync
                .checked_delta(before.root_parent_directory_sync)?,
            install_parent_directory_sync: self
                .install_parent_directory_sync
                .checked_delta(before.install_parent_directory_sync)?,
            dirty_tree_directory_sync: self
                .dirty_tree_directory_sync
                .checked_delta(before.dirty_tree_directory_sync)?,
            final_root_directory_sync: self
                .final_root_directory_sync
                .checked_delta(before.final_root_directory_sync)?,
            replace: self.replace.checked_delta(before.replace)?,
            authority_completion: self
                .authority_completion
                .checked_delta(before.authority_completion)?,
            cleanup: self.cleanup.checked_delta(before.cleanup)?,
        })
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            workspace_setup: self.workspace_setup.checked_add(other.workspace_setup)?,
            workspace_root_create_open: self
                .workspace_root_create_open
                .checked_add(other.workspace_root_create_open)?,
            staging_create_open: self
                .staging_create_open
                .checked_add(other.staging_create_open)?,
            recovery_marker_create: self
                .recovery_marker_create
                .checked_add(other.recovery_marker_create)?,
            name_preflight: self.name_preflight.checked_add(other.name_preflight)?,
            temp_create: self.temp_create.checked_add(other.temp_create)?,
            workspace_marker_write: self
                .workspace_marker_write
                .checked_add(other.workspace_marker_write)?,
            content_write: self.content_write.checked_add(other.content_write)?,
            metadata_value_write: self
                .metadata_value_write
                .checked_add(other.metadata_value_write)?,
            aggregate_native_write: self
                .aggregate_native_write
                .checked_add(other.aggregate_native_write)?,
            content_flush: self.content_flush.checked_add(other.content_flush)?,
            metadata_validate: self
                .metadata_validate
                .checked_add(other.metadata_validate)?,
            metadata_apply: self.metadata_apply.checked_add(other.metadata_apply)?,
            metadata_preinstall_verify: self
                .metadata_preinstall_verify
                .checked_add(other.metadata_preinstall_verify)?,
            metadata_postinstall_verify: self
                .metadata_postinstall_verify
                .checked_add(other.metadata_postinstall_verify)?,
            root_binding_revalidate: self
                .root_binding_revalidate
                .checked_add(other.root_binding_revalidate)?,
            regular_file_sync: self
                .regular_file_sync
                .checked_add(other.regular_file_sync)?,
            directory_sync: self.directory_sync.checked_add(other.directory_sync)?,
            recovery_marker_file_sync: self
                .recovery_marker_file_sync
                .checked_add(other.recovery_marker_file_sync)?,
            content_temp_file_sync: self
                .content_temp_file_sync
                .checked_add(other.content_temp_file_sync)?,
            post_hardlink_file_sync: self
                .post_hardlink_file_sync
                .checked_add(other.post_hardlink_file_sync)?,
            staging_directory_sync: self
                .staging_directory_sync
                .checked_add(other.staging_directory_sync)?,
            root_parent_directory_sync: self
                .root_parent_directory_sync
                .checked_add(other.root_parent_directory_sync)?,
            install_parent_directory_sync: self
                .install_parent_directory_sync
                .checked_add(other.install_parent_directory_sync)?,
            dirty_tree_directory_sync: self
                .dirty_tree_directory_sync
                .checked_add(other.dirty_tree_directory_sync)?,
            final_root_directory_sync: self
                .final_root_directory_sync
                .checked_add(other.final_root_directory_sync)?,
            replace: self.replace.checked_add(other.replace)?,
            authority_completion: self
                .authority_completion
                .checked_add(other.authority_completion)?,
            cleanup: self.cleanup.checked_add(other.cleanup)?,
        })
    }
}

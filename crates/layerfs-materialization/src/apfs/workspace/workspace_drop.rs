use super::*;

impl Drop for Workspace {
    fn drop(&mut self) {
        if let Some(staging) = self.staging_dir.take() {
            let start = Instant::now();
            let removed = super::ffi::remove_owned_tree(
                &staging,
                &self.staging_parent,
                &self.staging_name,
                &self.staging_identity,
            );
            finish_cleanup(&self.facts, start, removed.is_ok());
        }
    }
}

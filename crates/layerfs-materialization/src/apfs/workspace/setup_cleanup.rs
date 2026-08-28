use super::*;

impl Drop for SetupCleanup<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let Some(root_parent) = self.root_parent.as_ref() else {
            return;
        };
        if let Some(staging) = self.staging.as_mut().filter(|staging| staging.owned) {
            cleanup_setup_directory(self.facts, root_parent, staging);
        }
        if self.root.owned {
            cleanup_setup_directory(self.facts, root_parent, &mut self.root);
        }
    }
}

pub(super) fn cleanup_setup_directory(facts: &Recorder, parent: &File, entry: &mut SetupDirectory) {
    let start = Instant::now();
    let removed = (|| {
        let expected = entry
            .identity
            .as_deref()
            .ok_or_else(|| std::io::Error::from_raw_os_error(libc::ESTALE))?;
        if entry.file.is_none() {
            entry.file = Some(super::ffi::open_directory_at(parent, &entry.name)?);
        }
        let directory = entry.file.as_ref().expect("setup directory just opened");
        if super::ffi::file_stable_token(directory)? != expected {
            return Err(std::io::Error::from_raw_os_error(libc::ESTALE));
        }
        super::ffi::remove_owned_tree(directory, parent, &entry.name, expected)
    })();
    finish_cleanup(facts, start, removed.is_ok());
}

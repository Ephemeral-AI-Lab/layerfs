use std::path::{Path, PathBuf};

pub(crate) fn runtime_root(branch_store: &Path) -> PathBuf {
    let mut path = branch_store.as_os_str().to_owned();
    path.push(".runtime");
    path.into()
}

fn merge_directory_entries(
    old: impl Iterator<Item = CoreResult<(CanonicalName, InodeId)>>,
    new: impl Iterator<Item = CoreResult<(CanonicalName, InodeId)>>,
    visitor: &mut impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    let mut old = old;
    let mut new = new;
    let mut old_entry = old.next().transpose()?;
    let mut new_entry = new.next().transpose()?;
    loop {
        match (old_entry.take(), new_entry.take()) {
            (None, None) => return Ok(()),
            (Some((old_name, before)), Some((new_name, after))) if old_name == new_name => {
                if before != after {
                    visitor(DirectoryEntryDiff {
                        name: old_name,
                        before: Some(before),
                        after: Some(after),
                    })?;
                }
                old_entry = old.next().transpose()?;
                new_entry = new.next().transpose()?;
            }
            (Some((old_name, before)), Some((new_name, after))) if old_name < new_name => {
                visitor(DirectoryEntryDiff {
                    name: old_name,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
                new_entry = Some((new_name, after));
            }
            (Some((old_name, before)), Some((new_name, after))) => {
                visitor(DirectoryEntryDiff {
                    name: new_name,
                    before: None,
                    after: Some(after),
                })?;
                old_entry = Some((old_name, before));
                new_entry = new.next().transpose()?;
            }
            (Some((old_name, before)), None) => {
                visitor(DirectoryEntryDiff {
                    name: old_name,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
            }
            (None, Some((new_name, after))) => {
                visitor(DirectoryEntryDiff {
                    name: new_name,
                    before: None,
                    after: Some(after),
                })?;
                new_entry = new.next().transpose()?;
            }
        }
    }
}

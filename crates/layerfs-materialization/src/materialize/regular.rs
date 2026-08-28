use super::*;

pub(super) fn project_regular_file<T>(
    workspace: &dyn ProjectionWorkspace,
    parent: &dyn DirectoryHandle,
    name: &[u8],
    metadata: &NativeMetadata,
    requested_directory_durability: DirectoryDurability,
    write: impl FnOnce(&mut dyn Write) -> VfsResult<(T, u64)>,
    counters: &mut OperationCounters,
) -> VfsResult<T> {
    let mut temp = workspace.create_temp_at(parent)?;
    counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
    let mut output = BufWriter::with_capacity(1024 * 1024, temp.as_mut());
    let (result, written) = write(&mut output)?;
    output.flush()?;
    drop(output);
    counters.native.bytes_written = checked_add(counters.native.bytes_written, written)?;
    workspace.set_temp_metadata(temp.as_mut(), metadata)?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    let achieved_directory_durability = workspace.atomic_replace_with_directory_durability(
        temp,
        parent,
        name,
        requested_directory_durability,
    )?;
    counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
    let file_syncs = 1 + u64::from(metadata.bsd_flags != 0);
    let directory_syncs = u64::from(
        achieved_directory_durability == DirectoryDurability::ImmediateDirectoryDurability,
    );
    counters.native.sync_calls =
        checked_add(counters.native.sync_calls, file_syncs + directory_syncs)?;
    Ok(result)
}

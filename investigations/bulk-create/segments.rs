// Private staging diagnostic, NOT a FilesystemPort or Workspace implementation.
#[allow(dead_code)]
#[path = "../../benchmark/fs-bench-pro/workload.rs"]
mod workload;
use std::{fs::{self, File, OpenOptions}, os::unix::fs::{FileExt, MetadataExt}, path::PathBuf, time::Instant};
const SEGMENT: u64 = 4 * 1024 * 1024;
const CAP: u64 = 1024 * 1024 * 1024;
#[derive(Clone, Copy)]
struct Slice { segment: usize, offset: u64, len: usize }
struct Staging { root: PathBuf, files: Vec<(File, u64)>, appended: u64, allocated: u64 }
impl Staging {
    fn new(root: PathBuf) -> std::io::Result<Self> {
        fs::create_dir(&root)?;
        Ok(Self { root, files: Vec::new(), appended: 0, allocated: 0 })
    }
    fn append(&mut self, bytes: &[u8], fail: bool) -> std::io::Result<Slice> {
        if bytes.len() as u64 > SEGMENT || self.appended + bytes.len() as u64 > CAP {
            return Err(std::io::Error::other("staging limit"));
        }
        if self.files.last().is_none_or(|(_, end)| end + bytes.len() as u64 > SEGMENT) {
            let path = self.root.join(self.files.len().to_string());
            self.files.push((OpenOptions::new().create_new(true).read(true).write(true).open(path)?, 0));
        }
        let index = self.files.len()-1;
        let (file, end) = &mut self.files[index];
        let linked = fs::metadata(self.root.join(index.to_string()))?;
        let before = file.metadata()?;
        if (linked.dev(), linked.ino(), before.len()) != (before.dev(), before.ino(), *end) {
            return Err(std::io::Error::other("descriptor identity/high-water"));
        }
        let result = if fail {
            file.write_all_at(&bytes[..bytes.len()/2], *end).and_then(|_| Err(std::io::Error::other("injected partial append")))
        } else { file.write_all_at(bytes, *end) };
        let after = file.metadata()?; // Observe partial allocation before rollback.
        self.allocated = self.allocated - before.blocks()*512 + after.blocks()*512;
        if let Err(error) = result {
            file.set_len(*end)?;
            let restored = file.metadata()?;
            self.allocated = self.allocated - after.blocks()*512 + restored.blocks()*512;
            return Err(error);
        }
        let slice = Slice { segment: index, offset: *end, len: bytes.len() };
        *end += bytes.len() as u64;
        self.appended += bytes.len() as u64;
        Ok(slice)
    }
    fn read(&self, slice: Slice) -> std::io::Result<Vec<u8>> {
        let (file,end) = &self.files[slice.segment];
        assert!(slice.offset + slice.len as u64 <= *end);
        let mut bytes = vec![0; slice.len];
        file.read_exact_at(&mut bytes, slice.offset)?;
        Ok(bytes)
    }
    fn cleanup(mut self) -> std::io::Result<()> {
        // ponytail: append-only charged until End; per-segment liveness needed for sustained churn.
        self.files.clear();
        fs::remove_dir_all(self.root)
    }
}
fn main() -> workload::Result<()> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("scratch path")?);
    let mut check = Staging::new(root.with_extension("check"))?;
    let old = check.append(b"original", false)?;
    assert!(check.append(b"rejected", true).is_err());
    assert_eq!(check.appended, 8);
    let replacement = check.append(b"new", false)?;
    assert_eq!(check.read(old)?, b"original"); // Retained version/open handle remains valid.
    assert_eq!(check.read(replacement)?, b"new");
    let prefix = Slice { len: 3, ..old };
    assert_eq!(check.read(prefix)?, b"ori");
    check.cleanup()?;
    println!("focused_check=pass");
    let start=Instant::now();
    let entries = workload::workspace_common::shards(1, 500, "bulk")?;
    workload::workspace_common::validate_entries(&entries)?;
    println!("plan_ns={}",start.elapsed().as_nanos());
    let start=Instant::now();
    let mut staging=Staging::new(root)?;
    let mut records=Vec::with_capacity(100000);
    let mut buffer=vec![0;1024*1024];
    for entry in &entries {
        if let workload::workspace_common::EntryKind::File(content)=&entry.kind {
            let len=content.read_at(0,&mut buffer)?;
            assert_eq!(len as u64,content.len());
            records.push(staging.append(&buffer[..len],false)?);
        }
    }
    println!("generate_and_stage_ns={}\nfiles={}\nbytes={}\nsegments={}\nallocated_bytes={}",start.elapsed().as_nanos(),records.len(),staging.appended,staging.files.len(),staging.allocated);
    assert_eq!(records.len(),100000);
    assert_eq!(staging.appended,524288000);
    let start=Instant::now();
    for (slice, content) in records.iter().zip(entries.iter().filter_map(|entry| {
        if let workload::workspace_common::EntryKind::File(content)=&entry.kind {Some(content)} else {None}
    })) {
        let len=content.read_at(0,&mut buffer)?;
        assert_eq!(staging.read(*slice)?,buffer[..len]);
    }
    println!("independent_regenerate_compare_ns={}",start.elapsed().as_nanos());
    let start=Instant::now();
    staging.cleanup()?;
    println!("cleanup_ns={}\nstatus=pass",start.elapsed().as_nanos());
    Ok(())
}

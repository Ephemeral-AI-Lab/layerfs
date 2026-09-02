use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PREPEND: &[u8] = b"PREPEND010";

fn main() {
    if let Err(error) = run() {
        eprintln!("fs-benchmark-workload: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command] if command == "self-check" => self_check(),
        [command] if command == "noop" => Ok(()),
        [command, path] if command == "digest" => print_digest(path),
        [command, path] if command == "read" => print_read(path),
        [command, path] if command == "namespace-verify" => print_namespace(path),
        [command, fixture, path] if command == "create" => {
            let started = std::time::Instant::now();
            create(fixture, path)?;
            println!("inner_write_ns={}", started.elapsed().as_nanos());
            Ok(())
        }
        [command, path, index, base_size] if command == "edit" => {
            edit(path, index.parse()?, base_size.parse()?)
        }
        [command, path] if command == "prepend" => prepend(path),
        [command, path, expected_size, expected_digest] if command == "verify" => {
            let (size, digest) = digest(Path::new(path))?;
            if size != expected_size.parse::<u64>()? || digest != *expected_digest {
                return Err(format!(
                    "verification mismatch: size={size} sha256={digest} expected_size={expected_size} expected_sha256={expected_digest}"
                )
                .into());
            }
            println!("{size}\t{digest}");
            Ok(())
        }
        _ => Err("usage: fs-benchmark-workload self-check | digest|read|namespace-verify PATH | create FIXTURE PATH | edit PATH INDEX BASE_SIZE | prepend PATH | verify PATH SIZE SHA256".into()),
    }
}

fn create(fixture: impl AsRef<Path>, path: impl AsRef<Path>) -> Result<()> {
    let mut source = BufReader::with_capacity(1024 * 1024, File::open(fixture)?);
    let target = File::create(path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, target);
    std::io::copy(&mut source, &mut writer)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}

fn edit(path: impl AsRef<Path>, index: u64, base_size: u64) -> Result<()> {
    if base_size <= 10 {
        return Err("base size must exceed marker length".into());
    }
    let marker = format!("E{:09}", index.checked_add(1).ok_or("edit index overflow")?);
    if marker.len() != 10 {
        return Err("edit index exceeds the 10-byte marker".into());
    }
    let offset = index
        .checked_add(1)
        .and_then(|value| value.checked_mul(2_654_435_761))
        .ok_or("edit offset overflow")?
        % (base_size - marker.len() as u64);
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    write_all_at(&file, marker.as_bytes(), offset)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn write_all_at(file: &File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(bytes, offset)
}

#[cfg(not(unix))]
fn write_all_at(file: &File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut file = file;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(bytes)
}

fn prepend(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let temporary = path.with_extension("bin.prepend.tmp");
    let mut source = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let target = File::create(&temporary)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, target);
    writer.write_all(PREPEND)?;
    std::io::copy(&mut source, &mut writer)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn print_digest(path: impl AsRef<Path>) -> Result<()> {
    let (size, digest) = digest(path.as_ref())?;
    println!("{size}\t{digest}");
    Ok(())
}

fn print_read(path: impl AsRef<Path>) -> Result<()> {
    let mut input = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let bytes = std::io::copy(&mut input, &mut std::io::sink())?;
    println!("read_bytes={bytes}");
    Ok(())
}

fn print_namespace(path: impl AsRef<Path>) -> Result<()> {
    let summary = namespace_digest(path.as_ref())?;
    println!("regular_files={}", summary.regular_files);
    println!("data_directories={}", summary.data_directories);
    println!("logical_bytes={}", summary.logical_bytes);
    println!("namespace_digest={}", summary.digest);
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NamespaceSummary {
    regular_files: u64,
    data_directories: u64,
    logical_bytes: u64,
    digest: String,
}

fn namespace_digest(root: &Path) -> Result<NamespaceSummary> {
    if !root.is_dir() {
        return Err("namespace root is not a directory".into());
    }
    let mut entries = Vec::new();
    collect_namespace(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    let mut hash = Sha256::new();
    hash.update(b"layerfs/fs-bench-pro/namespace-tree/v1\0");
    let mut regular_files = 0_u64;
    let mut data_directories = 0_u64;
    let mut logical_bytes = 0_u64;
    let tasks = Arc::new(Mutex::new(
        entries
            .iter()
            .enumerate()
            .filter(|(_, (_, _, directory, _))| !directory)
            .map(|(index, (_, path, _, size))| (index, path.clone(), *size))
            .collect::<std::collections::VecDeque<_>>(),
    ));
    let workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(
            tasks
                .lock()
                .map_err(|_| "namespace task queue")?
                .len()
                .max(1),
        );
    let (sender, receiver) = mpsc::sync_channel::<std::result::Result<(usize, Vec<u8>), String>>(
        workers.saturating_mul(2),
    );
    std::thread::scope(|scope| -> Result<()> {
        for _ in 0..workers {
            let tasks = Arc::clone(&tasks);
            let sender = sender.clone();
            scope.spawn(move || loop {
                let task = match tasks.lock() {
                    Ok(mut tasks) => tasks.pop_front(),
                    Err(_) => {
                        let _ = sender.send(Err("namespace task queue".to_owned()));
                        return;
                    }
                };
                let Some((index, path, expected_size)) = task else {
                    return;
                };
                let result = (|| -> std::result::Result<_, String> {
                    let capacity = usize::try_from(expected_size)
                        .map_err(|_| "namespace file size overflow".to_owned())?;
                    let mut bytes = Vec::with_capacity(capacity);
                    File::open(path)
                        .and_then(|mut file| file.read_to_end(&mut bytes))
                        .map_err(|error| error.to_string())?;
                    if bytes.len() as u64 != expected_size {
                        return Err("namespace file changed during verification".to_owned());
                    }
                    Ok((index, bytes))
                })();
                if sender.send(result).is_err() {
                    return;
                }
            });
        }
        drop(sender);
        let mut pending = std::collections::BTreeMap::new();
        for (index, (relative, _, directory, expected_size)) in entries.iter().enumerate() {
            if *directory {
                data_directories = data_directories
                    .checked_add(1)
                    .ok_or("namespace directory count overflow")?;
                hash.update(b"D\0");
                hash.update(relative.as_bytes());
                hash.update(b"\0");
                continue;
            }
            while !pending.contains_key(&index) {
                let (ready, bytes) = receiver
                    .recv()
                    .map_err(|_| "namespace file reader stopped")?
                    .map_err(|error| -> Box<dyn Error> { error.into() })?;
                pending.insert(ready, bytes);
            }
            let bytes = pending
                .remove(&index)
                .ok_or("namespace file result ordering")?;
            regular_files = regular_files
                .checked_add(1)
                .ok_or("namespace file count overflow")?;
            logical_bytes = logical_bytes
                .checked_add(*expected_size)
                .ok_or("namespace logical byte overflow")?;
            hash.update(b"F\0");
            hash.update(relative.as_bytes());
            hash.update(b"\0");
            hash.update(expected_size.to_string().as_bytes());
            hash.update(b"\0");
            hash.update(&bytes);
            hash.update(b"\0");
        }
        Ok(())
    })?;
    Ok(NamespaceSummary {
        regular_files,
        data_directories,
        logical_bytes,
        digest: hex(&hash.finish()),
    })
}

fn collect_namespace(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf, bool, u64)>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)?
            .components()
            .map(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .ok_or("namespace path is not UTF-8")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("/");
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_dir() {
            output.push((relative, path.clone(), true, 0));
            collect_namespace(root, &path, output)?;
        } else if metadata.file_type().is_file() {
            output.push((relative, path, false, metadata.len()));
        } else {
            return Err("namespace contains a non-directory, non-regular entry".into());
        }
    }
    Ok(())
}

fn digest(path: &Path) -> Result<(u64, String)> {
    let mut input = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut bytes = [0_u8; 1024 * 1024];
    loop {
        let read = input.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        size = size.checked_add(read as u64).ok_or("file size overflow")?;
        hash.update(&bytes[..read]);
    }
    Ok((size, hex(&hash.finish())))
}

fn self_check() -> Result<()> {
    let root = std::env::temp_dir().join(format!(
        "fs-benchmark-pro-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&root)?;
    let checked = (|| -> Result<()> {
        let fixture = root.join("fixture.bin");
        let payload = root.join("payload.bin");
        let expected = (0_u16..8192).map(|value| value as u8).collect::<Vec<_>>();
        fs::write(&fixture, &expected)?;
        create(&fixture, &payload)?;
        let mut read = BufReader::with_capacity(1024, File::open(&payload)?);
        assert_eq!(
            std::io::copy(&mut read, &mut std::io::sink())?,
            expected.len() as u64
        );
        edit(&payload, 0, expected.len() as u64)?;
        prepend(&payload)?;
        let mut expected = expected;
        let marker = b"E000000001";
        let offset = 2_654_435_761_u64 % (expected.len() as u64 - marker.len() as u64);
        expected[offset as usize..offset as usize + marker.len()].copy_from_slice(marker);
        let expected = [PREPEND, &expected].concat();
        if fs::read(&payload)? != expected {
            return Err("workload byte oracle mismatch".into());
        }
        let (size, actual) = digest(&payload)?;
        let mut expected_hash = Sha256::new();
        expected_hash.update(&expected);
        if size != expected.len() as u64 || actual != hex(&expected_hash.finish()) {
            return Err("workload digest oracle mismatch".into());
        }
        let namespace = root.join("namespace");
        fs::create_dir(&namespace)?;
        fs::create_dir(namespace.join("d0000"))?;
        fs::write(namespace.join("d0000/f000000"), b"first")?;
        fs::write(namespace.join("d0000/f000001"), b"second")?;
        let before = namespace_digest(&namespace)?;
        if before.regular_files != 2
            || before.data_directories != 1
            || before.logical_bytes != 11
            || before != namespace_digest(&namespace)?
        {
            return Err("namespace digest oracle mismatch".into());
        }
        fs::write(namespace.join("d0000/f000001"), b"changed")?;
        if namespace_digest(&namespace)?.digest == before.digest {
            return Err("namespace digest missed a content change".into());
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(root);
    checked?;
    println!("{{\"schema\":\"fs-benchmark-pro-workload-self-check-v2\",\"status\":\"pass\"}}");
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("String formatting");
    }
    value
}

struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    buffered: usize,
    bytes: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            buffered: 0,
            bytes: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.bytes = self
            .bytes
            .checked_add(input.len() as u64)
            .expect("SHA-256 input size");
        if self.buffered != 0 {
            let take = (64 - self.buffered).min(input.len());
            self.block[self.buffered..self.buffered + take].copy_from_slice(&input[..take]);
            self.buffered += take;
            input = &input[take..];
            if self.buffered == 64 {
                compress(&mut self.state, &self.block);
                self.buffered = 0;
            }
        }
        while input.len() >= 64 {
            let block: &[u8; 64] = input[..64].try_into().expect("block length");
            compress(&mut self.state, block);
            input = &input[64..];
        }
        self.block[..input.len()].copy_from_slice(input);
        self.buffered = input.len();
    }

    fn finish(mut self) -> [u8; 32] {
        let bit_length = self.bytes.checked_mul(8).expect("SHA-256 bit length");
        self.block[self.buffered] = 0x80;
        self.buffered += 1;
        if self.buffered > 56 {
            self.block[self.buffered..].fill(0);
            compress(&mut self.state, &self.block);
            self.block = [0; 64];
        } else {
            self.block[self.buffered..56].fill(0);
        }
        self.block[56..].copy_from_slice(&bit_length.to_be_bytes());
        compress(&mut self.state, &self.block);
        let mut output = [0_u8; 32];
        for (chunk, word) in output.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        output
    }
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut words = [0_u32; 64];
    for (word, bytes) in words.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes(bytes.try_into().expect("word length"));
    }
    for index in 16..64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (word, constant) in words.into_iter().zip(K) {
        let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(sum1)
            .wrapping_add(choose)
            .wrapping_add(constant)
            .wrapping_add(word);
        let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = sum0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

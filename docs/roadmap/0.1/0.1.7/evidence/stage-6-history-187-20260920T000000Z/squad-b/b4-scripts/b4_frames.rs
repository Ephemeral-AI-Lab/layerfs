//! B4 sizing instrument: exact product-codec frame lengths + BLAKE3 object ids.
//!
//! Mirrors core/crates/layerfs-storage/src/encoding/codec.rs: level 3,
//! windowLog 18, contentSizeFlag 1, checksumFlag 1, dictIDFlag 0, nbWorkers 0,
//! static 2 MiB CCtx, ZSTD_CCtx_refPrefix for the prefix route.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::ptr;

const ENCODE_WORKSPACE_BYTES: usize = 2 * 1024 * 1024;
const WINDOW_LOG: i32 = 18;
const OBJECT_DOMAIN: &[u8] = b"layerfs/object/v2\0";

fn workspace(size: usize) -> Vec<u64> {
    vec![0u64; size.div_ceil(8)]
}

struct Ctx {
    memory: Vec<u64>,
    context: *mut zstd_sys::ZSTD_CCtx,
}

impl Ctx {
    fn new() -> Self {
        let mut memory = workspace(ENCODE_WORKSPACE_BYTES);
        let context = unsafe {
            zstd_sys::ZSTD_initStaticCCtx(
                memory.as_mut_ptr().cast::<std::ffi::c_void>(),
                std::mem::size_of_val(&memory[..]),
            )
        };
        assert!(!context.is_null(), "static cctx");
        Self { memory, context }
    }

    fn set_params(&self) {
        let context = self.context;
        unsafe {
            let r = zstd_sys::ZSTD_CCtx_reset(
                context,
                zstd_sys::ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            );
            assert_eq!(zstd_sys::ZSTD_isError(r), 0, "reset");
            for (parameter, value) in [
                (zstd_sys::ZSTD_cParameter::ZSTD_c_compressionLevel, 3),
                (zstd_sys::ZSTD_cParameter::ZSTD_c_windowLog, WINDOW_LOG),
                (zstd_sys::ZSTD_cParameter::ZSTD_c_contentSizeFlag, 1),
                (zstd_sys::ZSTD_cParameter::ZSTD_c_checksumFlag, 1),
                (zstd_sys::ZSTD_cParameter::ZSTD_c_dictIDFlag, 0),
                (zstd_sys::ZSTD_cParameter::ZSTD_c_nbWorkers, 0),
            ] {
                let r = zstd_sys::ZSTD_CCtx_setParameter(context, parameter, value);
                assert_eq!(zstd_sys::ZSTD_isError(r), 0, "setParameter");
            }
        }
    }

    fn frame_len(&mut self, raw: &[u8], prefix: Option<&[u8]>) -> usize {
        let context = self.context;
        self.set_params();
        let bound = unsafe { zstd_sys::ZSTD_compressBound(raw.len()) };
        assert_eq!(unsafe { zstd_sys::ZSTD_isError(bound) }, 0);
        let mut out = vec![0u8; bound];
        unsafe {
            if let Some(prefix) = prefix {
                let r = zstd_sys::ZSTD_CCtx_refPrefix(
                    context,
                    prefix.as_ptr().cast::<std::ffi::c_void>(),
                    prefix.len(),
                );
                assert_eq!(zstd_sys::ZSTD_isError(r), 0, "refPrefix");
            }
            let n = zstd_sys::ZSTD_compress2(
                context,
                out.as_mut_ptr().cast::<std::ffi::c_void>(),
                out.len(),
                raw.as_ptr().cast::<std::ffi::c_void>(),
                raw.len(),
            );
            if prefix.is_some() {
                let r = zstd_sys::ZSTD_CCtx_refPrefix(context, ptr::null(), 0);
                assert_eq!(zstd_sys::ZSTD_isError(r), 0, "clear prefix");
            }
            assert_eq!(zstd_sys::ZSTD_isError(n), 0, "compress2");
            n
        }
    }
}

fn read_blobs(path: &str) -> HashMap<String, Vec<u8>> {
    let file = std::fs::File::open(path).expect("blobs file");
    let mut out = HashMap::new();
    for line in io::BufReader::new(file).lines() {
        let line = line.expect("line");
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let id = parts.next().unwrap().to_string();
        let p = parts.next().unwrap();
        out.insert(id, std::fs::read(p).expect("blob read"));
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    if mode == "blake3" {
        let stdin = io::stdin();
        let mut out = io::BufWriter::new(io::stdout());
        for line in stdin.lock().lines() {
            let line = line.unwrap();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let id = parts.next().unwrap();
            let path = parts.next().unwrap();
            let bytes = std::fs::read(path).unwrap();
            let mut hasher = blake3::Hasher::new();
            hasher.update(OBJECT_DOMAIN);
            hasher.update(&bytes);
            writeln!(out, "{}\t{}", id, hasher.finalize().to_hex()).unwrap();
        }
        return;
    }
    assert_eq!(mode, "frames");
    let blobs = std::sync::Arc::new(read_blobs(&args[2]));
    let mut jobs: Vec<(String, String, Option<String>)> = Vec::new();
    {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line.unwrap();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let job = parts.next().unwrap().to_string();
            let target = parts.next().unwrap().to_string();
            let base = parts.next().unwrap();
            jobs.push((
                job,
                target,
                if base == "-" { None } else { Some(base.to_string()) },
            ));
        }
    }
    let jobs = std::sync::Arc::new(jobs);
    let workers: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(6);
    let next = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(usize, usize)>::new()));
    let mut handles = Vec::new();
    for _ in 0..workers {
        let jobs = jobs.clone();
        let blobs = blobs.clone();
        let next = next.clone();
        let results = results.clone();
        handles.push(std::thread::spawn(move || {
            let mut ctx = Ctx::new();
            let mut local = Vec::new();
            loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if i >= jobs.len() {
                    break;
                }
                let (_, target, base) = &jobs[i];
                let raw = &blobs[target];
                let prefix = base.as_ref().map(|b| blobs[b].as_slice());
                let n = ctx.frame_len(raw, prefix);
                local.push((i, n));
            }
            results.lock().unwrap().extend(local);
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let mut results = std::sync::Arc::try_unwrap(results)
        .unwrap()
        .into_inner()
        .unwrap();
    results.sort_unstable();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for (i, n) in results {
        writeln!(out, "{}\t{}", jobs[i].0, n).unwrap();
    }
}

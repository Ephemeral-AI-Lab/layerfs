
//! W1 scratch instrument: the exact zstd parameter matrix the product pins, on
//! real Store records. NOT product source; lives outside the repository.
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::io::{BufWriter, Write};
use std::ptr;

use zstd_sys::*;

fn now_cpu_ns() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
    (ts.tv_sec as u64) * 1_000_000_000 + (ts.tv_nsec as u64)
}

struct Rec {
    id: [u8; 32],
    canonical: u32,
    base: Option<[u8; 32]>,
    frame: Vec<u8>,
}

fn read_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn load(path: &str) -> Vec<Rec> {
    let b = fs::read(path).expect("records file");
    let count = read_u32(&b, 0) as usize;
    let mut o = 4usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut id = [0u8; 32];
        id.copy_from_slice(&b[o..o + 32]);
        o += 32;
        let canonical = read_u32(&b, o);
        o += 4;
        let mut bid = [0u8; 32];
        bid.copy_from_slice(&b[o..o + 32]);
        o += 32;
        let has_base = bid != [0u8; 32];
        let flen = read_u32(&b, o) as usize;
        o += 4;
        let frame = b[o..o + flen].to_vec();
        o += flen;
        out.push(Rec { id, canonical, base: if has_base { Some(bid) } else { None }, frame });
    }
    assert_eq!(o, b.len(), "records file trailing bytes");
    out
}

struct Enc {
    c: *mut ZSTD_CCtx,
    buf: Vec<u8>,
}

impl Enc {
    fn new() -> Self {
        let c = unsafe { ZSTD_createCCtx() };
        assert!(!c.is_null());
        Enc { c, buf: vec![0u8; 0] }
    }
    fn ensure(&mut self, n: usize) {
        if self.buf.len() < n {
            self.buf.resize(n, 0);
        }
    }
    fn reset(&mut self, level: i32, wlog: i32, ldm: bool, ldm_hash_log: i32, ldm_min_match: i32) {
        unsafe {
            let r = ZSTD_CCtx_reset(self.c, ZSTD_ResetDirective::ZSTD_reset_session_and_parameters);
            assert!(!is_err(r));
            let set = |p: ZSTD_cParameter, v: i32| {
                let r = ZSTD_CCtx_setParameter(self.c, p, v);
                if is_err(r) {
                    panic!("setParameter {:?}={} failed code {}", p, v, r);
                }
            };
            set(ZSTD_cParameter::ZSTD_c_compressionLevel, level);
            set(ZSTD_cParameter::ZSTD_c_windowLog, wlog);
            set(ZSTD_cParameter::ZSTD_c_contentSizeFlag, 1);
            set(ZSTD_cParameter::ZSTD_c_checksumFlag, 1);
            set(ZSTD_cParameter::ZSTD_c_dictIDFlag, 0);
            set(ZSTD_cParameter::ZSTD_c_nbWorkers, 0);
            if ldm {
                set(ZSTD_cParameter::ZSTD_c_enableLongDistanceMatching, 1);
                if ldm_hash_log > 0 {
                    set(ZSTD_cParameter::ZSTD_c_ldmHashLog, ldm_hash_log);
                }
                if ldm_min_match > 0 {
                    set(ZSTD_cParameter::ZSTD_c_ldmMinMatch, ldm_min_match);
                }
            }
        }
    }
    fn compress(&mut self, raw: &[u8], prefix: Option<&[u8]>) -> Vec<u8> {
        let bound = unsafe { ZSTD_compressBound(raw.len()) };
        self.ensure(bound);
        unsafe {
            if let Some(p) = prefix {
                let r = ZSTD_CCtx_refPrefix(self.c, p.as_ptr().cast::<c_void>(), p.len());
                assert!(!is_err(r));
            }
            let n = ZSTD_compress2(
                self.c,
                self.buf.as_mut_ptr().cast::<c_void>(),
                self.buf.len(),
                raw.as_ptr().cast::<c_void>(),
                raw.len(),
            );
            if is_err(n) {
                panic!("compress2 failed code {} for raw {} prefix {:?}", n, raw.len(), prefix.map(|p| p.len()));
            }
            let out = self.buf[..n].to_vec();
            if prefix.is_some() {
                let r = ZSTD_CCtx_refPrefix(self.c, ptr::null(), 0);
                assert!(!is_err(r));
            }
            out
        }
    }
}

impl Drop for Enc {
    fn drop(&mut self) {
        unsafe { ZSTD_freeCCtx(self.c) };
    }
}

fn is_err(c: usize) -> bool {
    unsafe { ZSTD_isError(c) != 0 }
}

struct Dec {
    d: *mut ZSTD_DCtx,
    buf: Vec<u8>,
}

impl Dec {
    fn new() -> Self {
        let d = unsafe { ZSTD_createDCtx() };
        assert!(!d.is_null());
        Dec { d, buf: Vec::new() }
    }
    fn decompress(&mut self, frame: &[u8], raw_len: usize, prefix: Option<&[u8]>) -> Vec<u8> {
        if self.buf.len() < raw_len {
            self.buf.resize(raw_len, 0);
        }
        unsafe {
            let r = ZSTD_DCtx_reset(self.d, ZSTD_ResetDirective::ZSTD_reset_session_and_parameters);
            assert!(!is_err(r));
            if let Some(p) = prefix {
                let r = ZSTD_DCtx_refPrefix(self.d, p.as_ptr().cast::<c_void>(), p.len());
                assert!(!is_err(r));
            }
            let n = ZSTD_decompressDCtx(
                self.d,
                self.buf.as_mut_ptr().cast::<c_void>(),
                self.buf.len(),
                frame.as_ptr().cast::<c_void>(),
                frame.len(),
            );
            if is_err(n) {
                panic!("decompress failed code {} frame {} raw_len {}", n, frame.len(), raw_len);
            }
            assert_eq!(n, raw_len, "decoded length mismatch");
            self.buf[..n].to_vec()
        }
    }
}

impl Drop for Dec {
    fn drop(&mut self) {
        unsafe { ZSTD_freeDCtx(self.d) };
    }
}

/// Decode one record's raw bytes, walking its base chain (depth is policy-capped).
fn raw_of(recs: &HashMap<[u8; 32], usize>, all: &[Rec], dec: &mut Dec, i: usize) -> Vec<u8> {
    let r = &all[i];
    match r.base {
        None => dec.decompress(&r.frame, (r.canonical - 23) as usize, None),
        Some(b) => {
            let bi = *recs.get(&b).expect("base not a whole-file object");
            let base_raw = raw_of(recs, all, dec, bi);
            dec.decompress(&r.frame, (r.canonical - 23) as usize, Some(&base_raw))
        }
    }
}

fn parse_csv(s: &str) -> Vec<i32> {
    s.split(',').map(|x| x.trim().parse().unwrap()).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args[1].as_str();
    if mode == "estimate" {
        let levels = parse_csv(&args[3]);
        let sizes = parse_csv(&args[4]);
        println!("level,src_size,window_log,chain_log,hash_log,search_log,min_match,strategy,est_cctx_bytes");
        for &lvl in &levels {
            for &s in &sizes {
                let p = unsafe { ZSTD_getCParams(lvl, s as u64, s as usize) };
                let est = unsafe { ZSTD_estimateCCtxSize_usingCParams(p) };
                println!(
                    "{},{},{},{},{},{},{},{:?},{}",
                    lvl, s, p.windowLog, p.chainLog, p.hashLog, p.searchLog, p.minMatch, p.strategy, est
                );
            }
        }
        return;
    }
    let recs = load(&args[2]);
    let index: HashMap<[u8; 32], usize> =
        recs.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    let mut dec = Dec::new();
    let mut enc = Enc::new();

    // Decode every record once; the raw population is what both regimes price.
    let mut raws: Vec<Vec<u8>> = Vec::with_capacity(recs.len());
    for i in 0..recs.len() {
        raws.push(raw_of(&index, &recs, &mut dec, i));
    }
    let bases: Vec<Option<Vec<u8>>> = recs
        .iter()
        .map(|r| r.base.map(|b| raw_of(&index, &recs, &mut dec, *index.get(&b).unwrap())))
        .collect();
    let baseless: Vec<usize> = (0..recs.len()).filter(|i| recs[*i].base.is_none()).collect();
    let based: Vec<usize> = (0..recs.len()).filter(|i| recs[*i].base.is_some()).collect();
    eprintln!(
        "records {} baseless {} ({}) based {} ({})",
        recs.len(),
        baseless.len(),
        baseless.iter().map(|i| recs[*i].frame.len()).sum::<usize>(),
        based.len(),
        based.iter().map(|i| recs[*i].frame.len()).sum::<usize>()
    );

    match mode {
        "levels" => {
            let levels = parse_csv(&args[3]);
            let wlog: i32 = args[4].parse().unwrap();
            println!("level,alone_baseless,alone_based,prefix_based,exact_full_frames,exact_prefix_frames,cpu_ns_alone_baseless,cpu_ns_alone_based,cpu_ns_prefix_based");
            for &lvl in &levels {
                let mut a = 0usize;
                let mut b = 0usize;
                let mut c = 0usize;
                let mut xf = 0usize;
                let mut xp = 0usize;
                enc.reset(lvl, wlog, false, 0, 0);
                let t0 = now_cpu_ns();
                for &i in &baseless {
                    let f = enc.compress(&raws[i], None);
                    if f == recs[i].frame {
                        xf += 1;
                    }
                    a += f.len();
                }
                let t1 = now_cpu_ns();
                for &i in &based {
                    b += enc.compress(&raws[i], None).len();
                }
                let t2 = now_cpu_ns();
                for &i in &based {
                    let f = enc.compress(&raws[i], bases[i].as_deref());
                    if f == recs[i].frame {
                        xp += 1;
                    }
                    c += f.len();
                }
                let t3 = now_cpu_ns();
                println!("{},{},{},{},{},{},{},{},{}", lvl, a, b, c, xf, xp, t1 - t0, t2 - t1, t3 - t2);
            }
        }
        "levels_min" => {
            // The product's own decision: a based record is stored as whichever
            // *complete record* is smaller - 1+frame FULL or 33+frame PREFIX.
            let levels = parse_csv(&args[3]);
            let wlog: i32 = args[4].parse().unwrap();
            println!("level,stored_bodies,flipped_to_full,cpu_ns");
            for &lvl in &levels {
                enc.reset(lvl, wlog, false, 0, 0);
                let t0 = now_cpu_ns();
                let mut total = 0usize;
                let mut flipped = 0usize;
                for &i in &baseless {
                    total += 1 + enc.compress(&raws[i], None).len();
                }
                for &i in &based {
                    let full = 1 + enc.compress(&raws[i], None).len();
                    let pre = 33 + enc.compress(&raws[i], bases[i].as_deref()).len();
                    if pre < full {
                        total += pre;
                    } else {
                        total += full;
                        flipped += 1;
                    }
                }
                let t1 = now_cpu_ns();
                println!("{},{},{},{}", lvl, total, flipped, t1 - t0);
            }
        }
        "window" => {
            let wlogs = parse_csv(&args[3]);
            let lvl: i32 = args[4].parse().unwrap();
            println!("window_log,alone_baseless,prefix_based,cpu_ns_prefix_based");
            for &w in &wlogs {
                enc.reset(lvl, w, false, 0, 0);
                let mut a = 0usize;
                let mut c = 0usize;
                for &i in &baseless {
                    a += enc.compress(&raws[i], None).len();
                }
                let t0 = now_cpu_ns();
                for &i in &based {
                    c += enc.compress(&raws[i], bases[i].as_deref()).len();
                }
                let t1 = now_cpu_ns();
                println!("{},{},{},{}", w, a, c, t1 - t0);
            }
        }
        "ldm" => {
            let lvl: i32 = args[3].parse().unwrap();
            let wlog: i32 = args[4].parse().unwrap();
            println!("ldm,ldm_hash_log,ldm_min_match,prefix_based,cpu_ns_prefix_based,alone_baseless");
            for (on, hl, mm) in [
                (false, 0, 0),
                (true, 0, 0),
                (true, 16, 0),
                (true, 20, 0),
                (true, 0, 32),
                (true, 20, 32),
            ] {
                enc.reset(lvl, wlog, on, hl, mm);
                let mut a = 0usize;
                for &i in &baseless {
                    a += enc.compress(&raws[i], None).len();
                }
                let t0 = now_cpu_ns();
                let mut c = 0usize;
                for &i in &based {
                    c += enc.compress(&raws[i], bases[i].as_deref()).len();
                }
                let t1 = now_cpu_ns();
                println!("{},{},{},{},{},{}", on, hl, mm, c, t1 - t0, a);
            }
        }
        "ldmsize" => {
            let lvl: i32 = args[3].parse().unwrap();
            let s: i32 = args[4].parse().unwrap();
            let w: i32 = args[5].parse().unwrap();
            let mut p = unsafe { ZSTD_getCParams(lvl, s as u64, s as usize) };
            p.windowLog = p.windowLog.min(w as u32);
            let est = unsafe { ZSTD_estimateCCtxSize_usingCParams(p) };
            println!("level {} src {} windowLog {} est {}", lvl, s, p.windowLog, est);
        }
        "groups" => {
            // args: mode groups <bodies.bin> <levels>
            let levels = parse_csv(&args[3]);
            let b = fs::read(&args[5]).expect("bodies");
            let count = read_u32(&b, 0) as usize;
            let mut o = 4usize;
            let mut bodies: Vec<Vec<u8>> = Vec::new();
            for _ in 0..count {
                let n = read_u32(&b, o) as usize;
                o += 4;
                bodies.push(b[o..o + n].to_vec());
                o += n;
            }
            assert_eq!(o, b.len());
            println!("level,stored_total,retained_frames,raw_total,cpu_ns,frame_total");
            for &lvl in &levels {
                let t0 = now_cpu_ns();
                let mut stored = 0usize;
                let mut frames = 0usize;
                let mut raw_total = 0usize;
                let mut kept = 0usize;
                for body in &bodies {
                    raw_total += body.len();
                    let mut p = unsafe { ZSTD_getCParams(lvl, body.len() as u64, 0usize) };
                    p.windowLog = p.windowLog.min(16u32);
                    unsafe {
                        let r = ZSTD_CCtx_reset(enc.c, ZSTD_ResetDirective::ZSTD_reset_session_and_parameters);
                        assert!(!is_err(r));
                        let r = ZSTD_CCtx_setCParams(enc.c, p);
                        assert!(!is_err(r));
                        let r = ZSTD_CCtx_setFParams(
                            enc.c,
                            ZSTD_frameParameters { contentSizeFlag: 1, checksumFlag: 1, noDictIDFlag: 1 },
                        );
                        assert!(!is_err(r));
                        let bound = ZSTD_compressBound(body.len());
                        enc.ensure(bound);
                        let n = ZSTD_compress2(
                            enc.c,
                            enc.buf.as_mut_ptr().cast::<c_void>(),
                            enc.buf.len(),
                            body.as_ptr().cast::<c_void>(),
                            body.len(),
                        );
                        if is_err(n) {
                            panic!("group compress failed {}", n);
                        }
                        frames += n;
                        if n + 16 <= body.len() {
                            kept += 1;
                            stored += n + 16;
                        } else {
                            stored += body.len() + 16;
                        }
                    }
                }
                let t1 = now_cpu_ns();
                println!("{},{},{},{},{},{}", lvl, stored, kept, raw_total, t1 - t0, frames);
            }
        }
        "levels_static" => {
            // Same sweep, but through a *static* context in a caller-owned
            // workspace of the given size, i.e. exactly what CompressionWorkspace
            // does. Answers: does the 2 MiB workspace degrade a high level?
            let levels = parse_csv(&args[3]);
            let wlog: i32 = args[4].parse().unwrap();
            let ws: usize = args[5].parse().unwrap();
            let mut mem: Vec<u64> = vec![0u64; ws.div_ceil(8)];
            let ctx = unsafe { ZSTD_initStaticCCtx(mem.as_mut_ptr().cast::<c_void>(), ws) };
            assert!(!ctx.is_null());
            println!("workspace,level,alone_baseless,prefix_based,exact_full_frames,exact_prefix_frames,cpu_ns_alone_baseless,cpu_ns_prefix_based");
            for &lvl in &levels {
                unsafe {
                    let set = |p: ZSTD_cParameter, v: i32| {
                        let r = ZSTD_CCtx_setParameter(ctx, p, v);
                        assert!(!is_err(r), "static setParameter {:?}={} -> {}", p, v, r);
                    };
                    let reset = || {
                        let r = ZSTD_CCtx_reset(ctx, ZSTD_ResetDirective::ZSTD_reset_session_and_parameters);
                        assert!(!is_err(r));
                    };
                    let mut a = 0usize;
                    let mut c = 0usize;
                    let mut xf = 0usize;
                    let mut xp = 0usize;
                    let mut bound = ZSTD_compressBound(131071);
                    let mut out = vec![0u8; bound];
                    let mut do_one = |raw: &[u8], prefix: Option<&[u8]>| -> Vec<u8> {
                        reset();
                        set(ZSTD_cParameter::ZSTD_c_compressionLevel, lvl);
                        set(ZSTD_cParameter::ZSTD_c_windowLog, wlog);
                        set(ZSTD_cParameter::ZSTD_c_contentSizeFlag, 1);
                        set(ZSTD_cParameter::ZSTD_c_checksumFlag, 1);
                        set(ZSTD_cParameter::ZSTD_c_dictIDFlag, 0);
                        set(ZSTD_cParameter::ZSTD_c_nbWorkers, 0);
                        if let Some(p) = prefix {
                            let r = ZSTD_CCtx_refPrefix(ctx, p.as_ptr().cast::<c_void>(), p.len());
                            assert!(!is_err(r));
                        }
                        if out.len() < bound {
                            out.resize(bound, 0);
                        }
                        let n = ZSTD_compress2(
                            ctx,
                            out.as_mut_ptr().cast::<c_void>(),
                            out.len(),
                            raw.as_ptr().cast::<c_void>(),
                            raw.len(),
                        );
                        assert!(!is_err(n), "static compress2 level {} -> {}", lvl, n);
                        let v = out[..n].to_vec();
                        if prefix.is_some() {
                            let r = ZSTD_CCtx_refPrefix(ctx, ptr::null(), 0);
                            assert!(!is_err(r));
                        }
                        v
                    };
                    let t0 = now_cpu_ns();
                    for &i in &baseless {
                        let f = do_one(&raws[i], None);
                        if f == recs[i].frame {
                            xf += 1;
                        }
                        a += f.len();
                    }
                    let t1 = now_cpu_ns();
                    for &i in &based {
                        let f = do_one(&raws[i], bases[i].as_deref());
                        if f == recs[i].frame {
                            xp += 1;
                        }
                        c += f.len();
                    }
                    let t2 = now_cpu_ns();
                    println!("{},{},{},{},{},{},{},{}", ws, lvl, a, c, xf, xp, t1 - t0, t2 - t1);
                    let _ = &mut bound;
                }
            }
        }
        "roundtrip" => {
            // Encode/decode verification at one level: every record must decode
            // back to its own raw bytes through the product's own decode shape.
            let lvl: i32 = args[3].parse().unwrap();
            let wlog: i32 = args[4].parse().unwrap();
            enc.reset(lvl, wlog, false, 0, 0);
            let mut ok = 0usize;
            let mut bad = 0usize;
            let mut bytes = 0usize;
            for i in 0..recs.len() {
                let f = enc.compress(&raws[i], bases[i].as_deref());
                bytes += f.len();
                let back = dec.decompress(&f, raws[i].len(), bases[i].as_deref());
                if back == raws[i] {
                    ok += 1;
                } else {
                    bad += 1;
                }
            }
            println!("level {} windowLog {} roundtrip_ok {} roundtrip_bad {} stored {}", lvl, wlog, ok, bad, bytes);
        }
        "ldmwin" => {
            // What does LDM do to the *frame header*? The product's read path
            // rejects any frame whose windowSize exceeds 1 << window_log.
            let lvl: i32 = args[3].parse().unwrap();
            let wlog: i32 = args[4].parse().unwrap();
            for on in [false, true] {
                enc.reset(lvl, wlog, on, 0, 0);
                let mut maxwin = 0u64;
                let mut nonsingle = 0usize;
                let mut total = 0usize;
                for &i in based.iter().take(2000) {
                    let f = enc.compress(&raws[i], bases[i].as_deref());
                    total += f.len();
                    let d = f[4];
                    let single = (d >> 5) & 1;
                    let fcs_flag = (d >> 6) & 3;
                    let did_flag = d & 3;
                    let fcs_size = match fcs_flag { 0 => if single == 1 { 1 } else { 0 }, 1 => 2, 2 => 4, _ => 8 };
                    let did_size = match did_flag { 0 => 0, 1 => 1, 2 => 2, _ => 4 };
                    let mut o = 5usize;
                    let mut win: u64 = 0;
                    if single == 1 {
                        nonsingle += 0;
                        let mut v = 0u64;
                        for k in 0..fcs_size {
                            v |= (f[o + k] as u64) << (8 * k);
                        }
                        if fcs_size == 2 {
                            v += 256;
                        }
                        win = v;
                    } else {
                        nonsingle += 1;
                        let wd = f[o];
                        let wl = 10 + (wd >> 3);
                        let base = 1u64 << wl;
                        win = base + (base / 8) * ((wd & 7) as u64);
                    }
                    o += fcs_size + did_size;
                    let _ = o;
                    if win > maxwin {
                        maxwin = win;
                    }
                }
                println!(
                    "ldm {} level {} windowLog {} : total {} maxWindowSize {} non_single_segment {} read_check_1<<{}={} rejects {}",
                    on, lvl, wlog, total, maxwin, nonsingle, wlog, 1u64 << wlog,
                    if maxwin > (1u64 << wlog) { "YES" } else { "no" }
                );
            }
        }
        "fit" => {
            // Smallest static workspace that actually holds the *real* parameter
            // sequence for (level, windowLog) over a source of srcSize bytes.
            let lvl: i32 = args[3].parse().unwrap();
            let wlog: i32 = args[4].parse().unwrap();
            let src: usize = args[5].parse().unwrap();
            let raw: Vec<u8> = (0..src).map(|i| ((i * 2654435761usize) >> 13) as u8).collect();
            for ws in [2097152usize, 4194304, 6291456, 8388608, 12582912, 16777216, 25165824] {
                let mut mem: Vec<u64> = vec![0u64; ws.div_ceil(8)];
                let ctx = unsafe { ZSTD_initStaticCCtx(mem.as_mut_ptr().cast::<c_void>(), ws) };
                assert!(!ctx.is_null());
                let ok = unsafe {
                    let r = ZSTD_CCtx_reset(ctx, ZSTD_ResetDirective::ZSTD_reset_session_and_parameters);
                    assert!(!is_err(r));
                    let mut err = 0usize;
                    for (p, v) in [
                        (ZSTD_cParameter::ZSTD_c_compressionLevel, lvl),
                        (ZSTD_cParameter::ZSTD_c_windowLog, wlog),
                        (ZSTD_cParameter::ZSTD_c_contentSizeFlag, 1),
                        (ZSTD_cParameter::ZSTD_c_checksumFlag, 1),
                        (ZSTD_cParameter::ZSTD_c_dictIDFlag, 0),
                        (ZSTD_cParameter::ZSTD_c_nbWorkers, 0),
                    ] {
                        let r = ZSTD_CCtx_setParameter(ctx, p, v);
                        if is_err(r) && err == 0 {
                            err = r;
                        }
                    }
                    let bound = ZSTD_compressBound(raw.len());
                    let mut out = vec![0u8; bound];
                    let n = ZSTD_compress2(
                        ctx,
                        out.as_mut_ptr().cast::<c_void>(),
                        out.len(),
                        raw.as_ptr().cast::<c_void>(),
                        raw.len(),
                    );
                    println!(
                        "level {} windowLog {} src {} workspace {} -> setParameter_err {} compress2 {} ok {}",
                        lvl, wlog, src, ws, err, n, !is_err(n)
                    );
                    !is_err(n)
                };
                if ok {
                    break;
                }
            }
        }
        "static" => {
            // Mirror CompressionWorkspace: one static CCtx inside a caller-owned
            // 2 MiB workspace, then set the level and compress one record.
            let levels = parse_csv(&args[3]);
            let ws = args[4].parse::<usize>().unwrap();
            let mut mem: Vec<u64> = vec![0u64; ws.div_ceil(8)];
            let ctx = unsafe {
                ZSTD_initStaticCCtx(mem.as_mut_ptr().cast::<c_void>(), ws)
            };
            println!("workspace {} static_ctx_null {}", ws, ctx.is_null());
            for &lvl in &levels {
                unsafe {
                    let r = ZSTD_CCtx_reset(ctx, ZSTD_ResetDirective::ZSTD_reset_session_and_parameters);
                    let mut first_err = 0usize;
                    let mut report = String::new();
                    for (p, v) in [
                        (ZSTD_cParameter::ZSTD_c_compressionLevel, lvl),
                        (ZSTD_cParameter::ZSTD_c_windowLog, 18),
                        (ZSTD_cParameter::ZSTD_c_contentSizeFlag, 1),
                        (ZSTD_cParameter::ZSTD_c_checksumFlag, 1),
                        (ZSTD_cParameter::ZSTD_c_dictIDFlag, 0),
                        (ZSTD_cParameter::ZSTD_c_nbWorkers, 0),
                    ] {
                        let r = ZSTD_CCtx_setParameter(ctx, p, v);
                        if is_err(r) && first_err == 0 {
                            first_err = r;
                        }
                        report.push_str(&format!("{:?}={} ", p, r));
                    }
                    let raw = &raws[0];
                    let bound = ZSTD_compressBound(raw.len());
                    let mut out = vec![0u8; bound];
                    let n = ZSTD_compress2(
                        ctx,
                        out.as_mut_ptr().cast::<c_void>(),
                        out.len(),
                        raw.as_ptr().cast::<c_void>(),
                        raw.len(),
                    );
                    println!(
                        "level {} reset_ok {} first_err {} compress2 {} is_err {} out {}",
                        lvl, !is_err(r), first_err, n, is_err(n), if is_err(n) { 0 } else { n }
                    );
                    let _ = report;
                }
            }
        }
        "raw" => {
            // dump decoded raw bytes for cross-checking (mode raw <records.bin> <out.bin>)
            let f = fs::File::create(&args[3]).unwrap();
            let mut w = BufWriter::new(f);
            for i in 0..recs.len() {
                w.write_all(&(raws[i].len() as u32).to_le_bytes()).unwrap();
                w.write_all(&raws[i]).unwrap();
            }
            w.flush().unwrap();
        }
        other => panic!("unknown mode {}", other),
    }
}

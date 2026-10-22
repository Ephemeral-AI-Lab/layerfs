//! Commit bundle codec (#420 bundle study). Mirrors encode.py exactly: the
//! history's bundles are produced by encode.py and decoded here, and the
//! C12 publish's bundle is encoded here; a full replay compared with the
//! fixture checks that both sides agree.
//!
//! Body (before zstd): 0xB1, flags (bit 0 colref, bit 1 global names), then
//! sections object_location, inode_value, binding_hint, file_digest,
//! content_signature, commit_lineage. See encode.py for the column layout.
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Ol {
    pub id: Vec<u8>,
    pub role: i64,
    pub clen: i64,
    pub pack: i64,
    pub group: i64,
    pub rec: i64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Iv {
    pub ordinal: i64,
    pub value: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Bh {
    pub child: i64,
    pub parent: i64,
    pub name: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Fd {
    pub digest: Vec<u8>,
    pub length: i64,
    pub root: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Cs {
    pub slot: i64,
    pub stamp: i64,
    pub object: Vec<u8>,
    pub signature: Vec<u8>,
}
/// The bundle's own Commit's lineage row (commit id implied).
#[derive(Clone, Debug, PartialEq)]
pub struct Cl {
    pub run: Vec<u8>,
    pub depth: i64,
    pub newest: i64,
    pub merged: Option<Vec<u8>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rows {
    pub ol: Vec<Ol>,
    pub iv: Vec<Iv>,
    pub bh: Vec<Bh>,
    pub fd: Vec<Fd>,
    pub cs: Vec<Cs>,
    pub cl: Vec<Cl>,
}
impl Rows {
    pub fn count(&self) -> usize {
        self.ol.len() + self.iv.len() + self.bh.len() + self.fd.len() + self.cs.len() + self.cl.len()
    }
}

/// Global numbering a colref bundle refers to. Implemented in memory and
/// over the host cache's side tables (apply.rs).
pub trait Refs {
    /// Objects numbered before this bundle.
    fn objects(&mut self) -> u64;
    fn object(&mut self, num: u64) -> Vec<u8>;
    fn object_num(&mut self, id: &[u8]) -> Option<u64>;
    fn names(&mut self) -> u64;
    fn name(&mut self, num: u64) -> Vec<u8>;
    fn name_num(&mut self, name: &[u8]) -> Option<u64>;
    fn commit(&mut self, bundle: u64) -> Vec<u8>;
    fn commit_bundle(&mut self, id: &[u8]) -> Option<u64>;
}

#[derive(Default)]
pub struct MemRefs {
    pub objects: Vec<Vec<u8>>,
    pub object_index: HashMap<Vec<u8>, u64>,
    pub names: Vec<Vec<u8>>,
    pub name_index: HashMap<Vec<u8>, u64>,
    pub commits: Vec<Vec<u8>>,
    pub commit_index: HashMap<Vec<u8>, u64>,
}
impl MemRefs {
    /// Record what a decoded/encoded bundle introduces.
    pub fn absorb(&mut self, rows: &Rows, new_names: &[Vec<u8>]) {
        for o in &rows.ol {
            if !self.object_index.contains_key(&o.id) {
                self.object_index.insert(o.id.clone(), self.objects.len() as u64);
                self.objects.push(o.id.clone());
            }
        }
        for n in new_names {
            if !self.name_index.contains_key(n) {
                self.name_index.insert(n.clone(), self.names.len() as u64);
                self.names.push(n.clone());
            }
        }
    }
}
impl Refs for MemRefs {
    fn objects(&mut self) -> u64 {
        self.objects.len() as u64
    }
    fn object(&mut self, num: u64) -> Vec<u8> {
        self.objects[num as usize].clone()
    }
    fn object_num(&mut self, id: &[u8]) -> Option<u64> {
        self.object_index.get(id).copied()
    }
    fn names(&mut self) -> u64 {
        self.names.len() as u64
    }
    fn name(&mut self, num: u64) -> Vec<u8> {
        self.names[num as usize].clone()
    }
    fn name_num(&mut self, name: &[u8]) -> Option<u64> {
        self.name_index.get(name).copied()
    }
    fn commit(&mut self, bundle: u64) -> Vec<u8> {
        self.commits[bundle as usize].clone()
    }
    fn commit_bundle(&mut self, id: &[u8]) -> Option<u64> {
        self.commit_index.get(id).copied()
    }
}

fn uv(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            out.push(b | 0x80);
        } else {
            out.push(b);
            return;
        }
    }
}
fn zz(out: &mut Vec<u8>, n: i64) {
    uv(out, ((n << 1) ^ (n >> 63)) as u64)
}

struct R<'a> {
    b: &'a [u8],
    at: usize,
}
impl<'a> R<'a> {
    fn uv(&mut self) -> u64 {
        let (mut n, mut shift) = (0u64, 0);
        loop {
            let b = self.b[self.at];
            self.at += 1;
            n |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return n;
            }
            shift += 7;
        }
    }
    fn i(&mut self) -> i64 {
        self.uv() as i64
    }
    fn zz(&mut self) -> i64 {
        let u = self.uv();
        ((u >> 1) as i64) ^ -((u & 1) as i64)
    }
    fn take(&mut self, n: usize) -> &'a [u8] {
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        s
    }
    fn byte(&mut self) -> u8 {
        self.take(1)[0]
    }
}

/// Decode a bundle body. `bundle` is its index (publication sequence);
/// returns the rows and the names it introduced (colref names mode).
pub fn decode(body: &[u8], bundle: u64, own_commit: &[u8], refs: &mut dyn Refs) -> (Rows, Vec<Vec<u8>>) {
    let mut r = R { b: body, at: 0 };
    assert_eq!(r.byte(), 0xB1, "bundle format");
    let flags = r.byte();
    let colref = flags & 1 != 0;
    let names_mode = flags & 2 != 0;
    // Basis: objects and names numbered, bundles published, at encoding.
    let (basis_objects, basis_names, basis_bundles) = if colref { (r.uv(), r.uv(), r.uv()) } else { (0, 0, 0) };
    let _ = bundle;
    let mut rows = Rows::default();
    // object_location
    let n = r.uv() as usize;
    let np = r.uv() as usize;
    let mut packs = Vec::with_capacity(np);
    let mut prev = 0i64;
    for _ in 0..np {
        prev += r.i();
        packs.push(prev);
    }
    let pidx: Vec<usize> = (0..n).map(|_| r.uv() as usize).collect();
    let groups: Vec<i64> = (0..n).map(|_| r.i()).collect();
    let mut recs = Vec::with_capacity(n);
    let mut pg: Option<(usize, i64)> = None;
    let mut pr = 0i64;
    for i in 0..n {
        let v = r.i();
        let rec = if pg == Some((pidx[i], groups[i])) { pr + v } else { v };
        pg = Some((pidx[i], groups[i]));
        pr = rec;
        recs.push(rec);
    }
    let mut roles = Vec::with_capacity(n);
    for _ in 0..n {
        let role = i64::from(r.byte());
        roles.push((role, r.i()));
    }
    for i in 0..n {
        rows.ol.push(Ol { id: r.take(32).to_vec(), role: roles[i].0, clen: roles[i].1, pack: packs[pidx[i]], group: groups[i], rec: recs[i] });
    }
    let base = basis_objects;
    let nol = n as u64;
    // Resolve a reference; None = a literal follows later.
    let resolve = |v: u64, rows: &Rows, refs: &mut dyn Refs| -> Option<Vec<u8>> {
        if v == 0 {
            None
        } else if v <= nol {
            Some(rows.ol[(v - 1) as usize].id.clone())
        } else {
            assert!(colref);
            Some(refs.object(base - 1 - (v - 1 - nol)))
        }
    };
    // inode_value
    let n = r.uv() as usize;
    let nruns = r.uv() as usize;
    let mut ordinals = Vec::with_capacity(n);
    let mut prev = 0i64;
    for _ in 0..nruns {
        let s = prev + r.zz();
        let len = r.i();
        ordinals.extend(s..s + len);
        prev = s + len;
    }
    let kinds: Vec<u8> = r.take(n).to_vec();
    let rcs: Vec<u64> = (0..n).map(|_| r.uv()).collect();
    let root_refs: Vec<u64> = (0..n).map(|_| r.uv()).collect();
    let inline = kinds.iter().filter(|k| *k & 0x80 != 0).count();
    let modes: Vec<u64> = (0..inline).map(|_| r.uv()).collect();
    let mut mtimes = Vec::with_capacity(inline);
    let mut pm = 0i64;
    for _ in 0..inline {
        pm += r.zz();
        mtimes.push(pm);
    }
    let nanos: Vec<u64> = (0..inline).map(|_| r.uv()).collect();
    let md_refs: Vec<u64> = (0..n - inline).map(|_| r.uv()).collect();
    let (mut ki, mut km) = (0, 0);
    for i in 0..n {
        let mut v = Vec::with_capacity(73);
        v.push(kinds[i]);
        v.extend_from_slice(&rcs[i].to_be_bytes());
        match resolve(root_refs[i], &rows, refs) {
            Some(id) => v.extend_from_slice(&id),
            None => v.extend_from_slice(r.take(32)),
        }
        if kinds[i] & 0x80 != 0 {
            v.extend_from_slice(&(modes[ki] as u32).to_be_bytes());
            v.extend_from_slice(&mtimes[ki].to_be_bytes());
            v.extend_from_slice(&(nanos[ki] as u32).to_be_bytes());
            v.extend_from_slice(&[0u8; 16]);
            ki += 1;
        } else {
            match resolve(md_refs[km], &rows, refs) {
                Some(id) => v.extend_from_slice(&id),
                None => v.extend_from_slice(r.take(32)),
            }
            km += 1;
        }
        rows.iv.push(Iv { ordinal: ordinals[i], value: v });
    }
    // binding_hint
    let n = r.uv() as usize;
    let mut children = Vec::with_capacity(n);
    let mut c = 0i64;
    for _ in 0..n {
        c += r.i();
        children.push(c);
    }
    let mut parents = Vec::with_capacity(n);
    let mut p = 0i64;
    for _ in 0..n {
        p += r.zz();
        parents.push(p);
    }
    // lens: (Some(len) = new literal name, None + num = known name)
    enum Nm {
        New(usize),
        Old(u64),
    }
    let names_before = basis_names;
    let mut new_names: Vec<Vec<u8>> = Vec::new();
    let mut lens = Vec::with_capacity(n);
    let mut count = names_before;
    for _ in 0..n {
        if names_mode {
            let v = r.uv();
            if v == 0 {
                lens.push(Nm::New(r.uv() as usize));
                count += 1;
            } else {
                lens.push(Nm::Old(count - v));
            }
        } else {
            lens.push(Nm::New(r.uv() as usize));
        }
    }
    for i in 0..n {
        let name = match lens[i] {
            Nm::New(len) => {
                let name = r.take(len).to_vec();
                if names_mode {
                    new_names.push(name.clone());
                }
                name
            }
            Nm::Old(num) => {
                if num >= names_before {
                    new_names[(num - names_before) as usize].clone()
                } else {
                    refs.name(num)
                }
            }
        };
        rows.bh.push(Bh { child: children[i], parent: parents[i], name });
    }
    // file_digest
    let n = r.uv() as usize;
    let lengths: Vec<i64> = (0..n).map(|_| r.i()).collect();
    let rrefs: Vec<u64> = (0..n).map(|_| r.uv()).collect();
    let digests: Vec<Vec<u8>> = (0..n).map(|_| r.take(32).to_vec()).collect();
    for i in 0..n {
        let root = match resolve(rrefs[i], &rows, refs) {
            Some(id) => id,
            None => r.take(32).to_vec(),
        };
        rows.fd.push(Fd { digest: digests[i].clone(), length: lengths[i], root });
    }
    // content_signature
    let n = r.uv() as usize;
    let mut slot = 0i64;
    for _ in 0..n {
        slot += r.zz();
        let stamp = r.i();
        let v = r.uv();
        let object = match resolve(v, &rows, refs) {
            Some(id) => id,
            None => r.take(32).to_vec(),
        };
        rows.cs.push(Cs { slot, stamp, object, signature: r.take(32).to_vec() });
    }
    // commit_lineage
    let n = r.uv() as usize;
    for _ in 0..n {
        let run = match r.uv() {
            0 => own_commit.to_vec(),
            1 => r.take(33).to_vec(),
            v => refs.commit(basis_bundles + 1 - v),
        };
        let depth = r.i();
        let newest = r.i();
        let merged = match r.uv() {
            0 => None,
            1 => Some(r.take(33).to_vec()),
            v => Some(refs.commit(basis_bundles + 1 - v)),
        };
        rows.cl.push(Cl { run, depth, newest, merged });
    }
    assert_eq!(r.at, body.len(), "bundle trailing bytes");
    (rows, new_names)
}

/// Encode rows as encode.py does (the rows are sorted here).
/// `names`: number binding names globally (colref only).
pub fn encode(rows: &Rows, bundle: u64, own_commit: &[u8], colref: bool, names: bool, refs: &mut dyn Refs) -> (Vec<u8>, Vec<Vec<u8>>) {
    let names_mode = colref && names;
    let mut out = vec![0xB1, u8::from(colref) | if names_mode { 2 } else { 0 }];
    // `bundle` is the basis: bundles published when encoding (own index if
    // nothing intervenes); refs to earlier bundles are relative to it.
    if colref {
        let (o, n) = (refs.objects(), refs.names());
        uv(&mut out, o);
        uv(&mut out, n);
        uv(&mut out, bundle);
    }
    let mut ol = rows.ol.clone();
    ol.sort_by(|a, b| (a.pack, a.group, a.rec, &a.id).cmp(&(b.pack, b.group, b.rec, &b.id)));
    let local: HashMap<&[u8], u64> = ol.iter().enumerate().map(|(i, o)| (&o.id[..], i as u64)).collect();
    let base = refs.objects();
    let nol = ol.len() as u64;
    let idref = |id: &[u8], refs: &mut dyn Refs, out: &mut Vec<u8>, lits: &mut Vec<u8>| {
        if let Some(i) = local.get(id) {
            uv(out, 1 + i);
        } else if let Some(num) = if colref { refs.object_num(id) } else { None } {
            uv(out, 1 + nol + (base - 1 - num));
        } else {
            out.push(0);
            lits.extend_from_slice(id);
        }
    };
    let mut packs: Vec<i64> = ol.iter().map(|o| o.pack).collect();
    packs.dedup();
    uv(&mut out, nol);
    uv(&mut out, packs.len() as u64);
    let mut prev = 0;
    for p in &packs {
        uv(&mut out, (p - prev) as u64);
        prev = *p;
    }
    let pidx: HashMap<i64, u64> = packs.iter().enumerate().map(|(i, p)| (*p, i as u64)).collect();
    let mut cols: [Vec<u8>; 5] = Default::default();
    let mut pg: Option<(i64, i64)> = None;
    let mut pr = 0i64;
    for o in &ol {
        uv(&mut cols[0], pidx[&o.pack]);
        uv(&mut cols[1], o.group as u64);
        let same = pg == Some((o.pack, o.group)) && o.rec >= pr;
        uv(&mut cols[2], if same { o.rec - pr } else { o.rec } as u64);
        pg = Some((o.pack, o.group));
        pr = o.rec;
        cols[3].push(o.role as u8);
        uv(&mut cols[3], o.clen as u64);
        cols[4].extend_from_slice(&o.id);
    }
    for c in &cols {
        out.extend_from_slice(c);
    }
    // inode_value
    let mut iv = rows.iv.clone();
    iv.sort_by_key(|v| v.ordinal);
    uv(&mut out, iv.len() as u64);
    let mut runs: Vec<(i64, i64)> = Vec::new();
    for v in &iv {
        match runs.last_mut() {
            Some((s, n)) if *s + *n == v.ordinal => *n += 1,
            _ => runs.push((v.ordinal, 1)),
        }
    }
    uv(&mut out, runs.len() as u64);
    let mut prev = 0;
    for (s, n) in &runs {
        zz(&mut out, s - prev);
        uv(&mut out, *n as u64);
        prev = s + n;
    }
    let (mut kinds, mut rcs, mut roots, mut lits, mut md) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut modes, mut mtimes, mut nanos) = (Vec::new(), Vec::new(), Vec::new());
    let mut pm = 0i64;
    for v in &iv {
        let b = &v.value;
        kinds.push(b[0]);
        uv(&mut rcs, u64::from_be_bytes(b[1..9].try_into().unwrap()));
        idref(&b[9..41], refs, &mut roots, &mut lits);
        if b[0] & 0x80 != 0 {
            uv(&mut modes, u64::from(u32::from_be_bytes(b[41..45].try_into().unwrap())));
            let ms = i64::from_be_bytes(b[45..53].try_into().unwrap());
            zz(&mut mtimes, ms - pm);
            pm = ms;
            uv(&mut nanos, u64::from(u32::from_be_bytes(b[53..57].try_into().unwrap())));
        } else {
            idref(&b[41..73], refs, &mut md, &mut lits);
        }
    }
    for part in [kinds, rcs, roots, modes, mtimes, nanos, md, lits] {
        out.extend_from_slice(&part);
    }
    // binding_hint, sorted by (child, parent, name)
    let mut bh = rows.bh.clone();
    bh.sort_by(|a, b| (a.child, a.parent, &a.name).cmp(&(b.child, b.parent, &b.name)));
    uv(&mut out, bh.len() as u64);
    let (mut a, mut p, mut lens, mut names) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut pc, mut pp) = (0i64, 0i64);
    let mut new_names: Vec<Vec<u8>> = Vec::new();
    let mut local_names: HashMap<Vec<u8>, u64> = HashMap::new();
    let names_before = refs.names();
    for h in &bh {
        uv(&mut a, (h.child - pc) as u64);
        zz(&mut p, h.parent - pp);
        pc = h.child;
        pp = h.parent;
        if names_mode {
            let count = names_before + new_names.len() as u64;
            let known = local_names.get(&h.name).copied().or_else(|| refs.name_num(&h.name));
            match known {
                Some(num) => uv(&mut lens, count - num),
                None => {
                    lens.push(0);
                    uv(&mut lens, h.name.len() as u64);
                    names.extend_from_slice(&h.name);
                    local_names.insert(h.name.clone(), count);
                    new_names.push(h.name.clone());
                }
            }
        } else {
            uv(&mut lens, h.name.len() as u64);
            names.extend_from_slice(&h.name);
        }
    }
    for part in [a, p, lens, names] {
        out.extend_from_slice(&part);
    }
    // file_digest, sorted by digest
    let mut fd = rows.fd.clone();
    fd.sort_by(|a, b| (&a.digest, a.length).cmp(&(&b.digest, b.length)));
    uv(&mut out, fd.len() as u64);
    let (mut ls, mut rs, mut ds, mut rl) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for d in &fd {
        uv(&mut ls, d.length as u64);
        idref(&d.root, refs, &mut rs, &mut rl);
        ds.extend_from_slice(&d.digest);
    }
    for part in [ls, rs, ds, rl] {
        out.extend_from_slice(&part);
    }
    // content_signature, sorted by slot
    let mut cs = rows.cs.clone();
    cs.sort_by_key(|c| c.slot);
    uv(&mut out, cs.len() as u64);
    let mut ps = 0;
    for c in &cs {
        zz(&mut out, c.slot - ps);
        ps = c.slot;
        uv(&mut out, c.stamp as u64);
        let mut lit = Vec::new();
        idref(&c.object, refs, &mut out, &mut lit);
        out.extend_from_slice(&lit);
        out.extend_from_slice(&c.signature);
    }
    // commit_lineage
    uv(&mut out, rows.cl.len() as u64);
    for l in &rows.cl {
        if l.run == own_commit {
            out.push(0);
        } else if let Some(b) = if colref { refs.commit_bundle(&l.run) } else { None } {
            uv(&mut out, 1 + (bundle - b));
        } else {
            out.push(1);
            out.extend_from_slice(&l.run);
        }
        uv(&mut out, l.depth as u64);
        uv(&mut out, l.newest as u64);
        match &l.merged {
            None => out.push(0),
            Some(m) => match if colref { refs.commit_bundle(m) } else { None } {
                Some(b) => uv(&mut out, 1 + (bundle - b)),
                None => {
                    out.push(1);
                    out.extend_from_slice(m);
                }
            },
        }
    }
    (out, new_names)
}

// zstd through zstd-sys (the product's own zstd dependency).
use std::ffi::c_void;
use zstd_sys as z;

pub fn compress(src: &[u8], level: i32, dict: Option<&[u8]>) -> Vec<u8> {
    unsafe {
        let cap = z::ZSTD_compressBound(src.len());
        let mut out: Vec<u8> = Vec::with_capacity(cap);
        let n = match dict {
            None => z::ZSTD_compress(out.as_mut_ptr() as *mut c_void, cap, src.as_ptr() as *const c_void, src.len(), level),
            Some(d) => {
                let cctx = z::ZSTD_createCCtx();
                let n = z::ZSTD_compress_usingDict(cctx, out.as_mut_ptr() as *mut c_void, cap, src.as_ptr() as *const c_void, src.len(), d.as_ptr() as *const c_void, d.len(), level);
                z::ZSTD_freeCCtx(cctx);
                n
            }
        };
        assert_eq!(z::ZSTD_isError(n), 0, "zstd compress");
        out.set_len(n);
        out
    }
}

/// A prepared decompression dictionary (kept for the life of a host).
pub struct DDict(*mut z::ZSTD_DDict, *mut z::ZSTD_DCtx);
impl DDict {
    pub fn new(dict: Option<&[u8]>) -> Self {
        unsafe {
            let d = match dict {
                Some(d) => z::ZSTD_createDDict(d.as_ptr() as *const c_void, d.len()),
                None => std::ptr::null_mut(),
            };
            DDict(d, z::ZSTD_createDCtx())
        }
    }
    pub fn decompress(&self, src: &[u8]) -> Vec<u8> {
        unsafe {
            let size = z::ZSTD_getFrameContentSize(src.as_ptr() as *const c_void, src.len());
            assert!(size < u64::MAX - 2, "zstd frame size unknown");
            let mut out: Vec<u8> = Vec::with_capacity(size as usize);
            let n = if self.0.is_null() {
                z::ZSTD_decompressDCtx(self.1, out.as_mut_ptr() as *mut c_void, size as usize, src.as_ptr() as *const c_void, src.len())
            } else {
                z::ZSTD_decompress_usingDDict(self.1, out.as_mut_ptr() as *mut c_void, size as usize, src.as_ptr() as *const c_void, src.len(), self.0)
            };
            assert_eq!(z::ZSTD_isError(n), 0, "zstd decompress");
            out.set_len(n);
            out
        }
    }
}
impl Drop for DDict {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                z::ZSTD_freeDDict(self.0);
            }
            z::ZSTD_freeDCtx(self.1);
        }
    }
}

/// Bundle bodies as written by encode.py: [u32 LE length][body]...
pub fn read_bodies(path: &str) -> Vec<Vec<u8>> {
    let all = std::fs::read(path).unwrap();
    let mut out = Vec::new();
    let mut at = 0;
    while at < all.len() {
        let n = u32::from_le_bytes(all[at..at + 4].try_into().unwrap()) as usize;
        out.push(all[at + 4..at + 4 + n].to_vec());
        at += 4 + n;
    }
    out
}
pub fn read_commits(path: &str) -> Vec<Vec<u8>> {
    std::fs::read(path).unwrap().chunks(33).map(<[u8]>::to_vec).collect()
}

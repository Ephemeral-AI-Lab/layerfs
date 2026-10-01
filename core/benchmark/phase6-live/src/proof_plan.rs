//! Exact known publication replies; not independent canonical expected pins.
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
#[derive(Clone)]
pub struct Head {
    pub id: [u8; 33],
    pub root: [u8; 32],
    pub parent: Option<[u8; 33]>,
}
pub struct Plan {
    pub scenario: Option<[u8; 32]>,
    pub branch: [u8; 17],
    pub head: Option<[u8; 33]>,
    pub roots: Vec<[u8; 32]>,
    pub commits: Vec<Head>,
}
fn get<const N: usize>(r: &mut impl Read) -> Result<[u8; N], String> {
    let mut b = [0; N];
    r.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}
fn optional(r: &mut impl Read) -> Result<Option<[u8; 33]>, String> {
    match get::<1>(r)?[0] {
        0 => Ok(None),
        1 => get(r).map(Some),
        _ => Err("proof presence flag".into()),
    }
}
fn count(r: &mut impl Read) -> Result<usize, String> {
    let n = u32::from_be_bytes(get(r)?) as usize;
    if n > 8 {
        return Err("proof count admission".into());
    }
    Ok(n)
}
fn put_optional(w: &mut impl Write, value: Option<[u8; 33]>) -> Result<(), String> {
    w.write_all(&[u8::from(value.is_some())])
        .map_err(|e| e.to_string())?;
    if let Some(v) = value {
        w.write_all(&v).map_err(|e| e.to_string())?
    }
    Ok(())
}
impl Plan {
    pub fn write(&self, path: &Path) -> Result<(), String> {
        if self.roots.is_empty() || self.roots.len() > 8 || self.commits.len() > 8 {
            return Err("proof plan admission".into());
        }
        let mut f = File::create_new(path).map_err(|e| e.to_string())?;
        f.write_all(b"P6PROOF1").map_err(|e| e.to_string())?;
        f.write_all(&[u8::from(self.scenario.is_some())])
            .map_err(|e| e.to_string())?;
        if let Some(h) = self.scenario {
            f.write_all(&h).map_err(|e| e.to_string())?
        }
        f.write_all(&self.branch).map_err(|e| e.to_string())?;
        put_optional(&mut f, self.head)?;
        f.write_all(&(self.roots.len() as u32).to_be_bytes())
            .map_err(|e| e.to_string())?;
        for r in &self.roots {
            f.write_all(r).map_err(|e| e.to_string())?
        }
        f.write_all(&(self.commits.len() as u32).to_be_bytes())
            .map_err(|e| e.to_string())?;
        for h in &self.commits {
            f.write_all(&h.id).map_err(|e| e.to_string())?;
            f.write_all(&h.root).map_err(|e| e.to_string())?;
            put_optional(&mut f, h.parent)?
        }
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        if f.metadata().map_err(|e| e.to_string())?.len() > 4096 {
            return Err("proof bytes admission".into());
        }
        if get::<8>(&mut f)? != *b"P6PROOF1" {
            return Err("proof framing".into());
        }
        let scenario = match get::<1>(&mut f)?[0] {
            0 => None,
            1 => Some(get(&mut f)?),
            _ => return Err("proof scenario flag".into()),
        };
        let branch = get(&mut f)?;
        let head = optional(&mut f)?;
        let n = count(&mut f)?;
        if n == 0 {
            return Err("proof empty roots".into());
        }
        let mut roots = Vec::new();
        for _ in 0..n {
            roots.push(get(&mut f)?)
        }
        let n = count(&mut f)?;
        let mut commits = Vec::new();
        for _ in 0..n {
            commits.push(Head {
                id: get(&mut f)?,
                root: get(&mut f)?,
                parent: optional(&mut f)?,
            })
        }
        if f.read(&mut [0]).map_err(|e| e.to_string())? != 0 {
            return Err("proof trailing bytes".into());
        }
        Ok(Self {
            scenario,
            branch,
            head,
            roots,
            commits,
        })
    }
}

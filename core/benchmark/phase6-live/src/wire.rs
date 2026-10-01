use crate::objects::{Locator, ROLES};
use layerfs_content::ObjectId;
pub const PREFIX: &[u8] = b"P6META6";
pub struct Bytes<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Bytes<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    pub fn take<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let out = self
            .bytes
            .get(self.at..self.at + N)
            .ok_or("metadata EOF")?
            .try_into()
            .map_err(|_| "metadata width")?;
        self.at += N;
        Ok(out)
    }
    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take::<1>()?[0])
    }
    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take()?))
    }
    pub fn count(&mut self) -> Result<usize, String> {
        let n = u16::from_be_bytes(self.take()?) as usize;
        if !(1..=128).contains(&n) {
            return Err("batch count".into());
        }
        Ok(n)
    }
    pub fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(self.take()?))
    }
    pub fn blob(&mut self) -> Result<Vec<u8>, String> {
        let n = u16::from_be_bytes(self.take()?) as usize;
        if n > 4096 {
            return Err("field capacity".into());
        }
        let b = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or("field EOF")?
            .to_vec();
        self.at += n;
        Ok(b)
    }
    pub fn string(&mut self) -> Result<String, String> {
        String::from_utf8(self.blob()?).map_err(|_| "metadata UTF8".into())
    }
    pub fn done(self) -> Result<(), String> {
        if self.at != self.bytes.len() {
            Err("metadata trailing bytes".into())
        } else {
            Ok(())
        }
    }
}
pub fn blob(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value)
}
#[derive(Clone)]
pub struct Snapshot {
    pub stack: [u8; 17],
    pub branch: [u8; 17],
    pub base: [u8; 33],
    pub head: Option<[u8; 33]>,
    pub root: [u8; 32],
    pub scope: [u8; 32],
    pub profile: [u8; 32],
}
impl Snapshot {
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.stack);
        out.extend_from_slice(&self.branch);
        out.extend_from_slice(&self.base);
        out.push(u8::from(self.head.is_some()));
        if let Some(id) = self.head {
            out.extend_from_slice(&id)
        }
        out.extend_from_slice(&self.root);
        out.extend_from_slice(&self.scope);
        out.extend_from_slice(&self.profile)
    }
    pub fn read(b: &mut Bytes<'_>) -> Result<Self, String> {
        Ok(Self {
            stack: b.take()?,
            branch: b.take()?,
            base: b.take()?,
            head: match b.u8()? {
                0 => None,
                1 => Some(b.take()?),
                _ => return Err("optional head tag".into()),
            },
            root: b.take()?,
            scope: b.take()?,
            profile: b.take()?,
        })
    }
}
pub fn locator(out: &mut Vec<u8>, row: &Locator) {
    out.extend_from_slice(row.id.as_bytes());
    out.push(ROLES.iter().position(|r| *r == row.role).unwrap() as u8);
    out.extend_from_slice(&(row.length as u64).to_be_bytes());
    out.extend_from_slice(&row.pack);
    out.extend_from_slice(&row.group.to_be_bytes());
    out.extend_from_slice(&row.record.to_be_bytes());
    out.extend_from_slice(&row.pack_id.to_be_bytes());
}
pub fn read_locator(b: &mut Bytes<'_>) -> Result<Locator, String> {
    Ok(Locator {
        id: ObjectId::from_bytes(&b.take::<32>()?).map_err(|e| e.to_string())?,
        role: *ROLES.get(b.u8()? as usize).ok_or("object role")?,
        length: usize::try_from(b.u64()?).map_err(|_| "canonical length")?,
        pack: b.take()?,
        group: b.u32()?,
        record: b.u32()?,
        pack_id: b.u64()?,
    })
}

//! Shallow negotiated Engine schema, streamed without collecting unknown values.
use crate::RuntimeError;
use std::io::Read;
pub(super) struct Json<R> {
    input: R,
    peeked: Option<u8>,
}
impl<R: Read> Json<R> {
    pub fn new(input: R) -> Self {
        Self {
            input,
            peeked: None,
        }
    }
    pub fn peek(&mut self) -> Result<Option<u8>, RuntimeError> {
        if self.peeked.is_none() {
            let mut one = [0];
            if self.input.read(&mut one)? == 0 {
                return Ok(None);
            }
            self.peeked = Some(one[0]);
        }
        Ok(self.peeked)
    }
    pub fn byte(&mut self) -> Result<u8, RuntimeError> {
        self.peek()?
            .ok_or(RuntimeError::Protocol("truncated JSON"))?;
        Ok(self.peeked.take().expect("peeked"))
    }
    pub fn expect(&mut self, b: u8) -> Result<(), RuntimeError> {
        if self.byte()? != b {
            return Err(RuntimeError::Protocol("JSON punctuation"));
        }
        Ok(())
    }
    pub fn whitespace(&mut self) -> Result<(), RuntimeError> {
        while matches!(self.peek()?, Some(b' ' | b'\r' | b'\n' | b'\t')) {
            self.byte()?;
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), RuntimeError> {
        self.whitespace()?;
        if self.peek()?.is_some() {
            return Err(RuntimeError::Protocol("trailing JSON value"));
        }
        Ok(())
    }
    pub fn object(
        &mut self,
        mut field: impl FnMut(&mut Self, &super::strings::Text) -> Result<(), RuntimeError>,
    ) -> Result<(), RuntimeError> {
        self.whitespace()?;
        self.expect(b'{')?;
        self.whitespace()?;
        if self.peek()? == Some(b'}') {
            self.byte()?;
            return Ok(());
        }
        loop {
            let key = self.string(true)?;
            self.whitespace()?;
            self.expect(b':')?;
            self.whitespace()?;
            field(self, &key)?;
            self.whitespace()?;
            match self.byte()? {
                b'}' => return Ok(()),
                b',' => {
                    self.whitespace()?;
                }
                _ => return Err(RuntimeError::Protocol("JSON object separator")),
            }
        }
    }
    pub fn boolean(&mut self) -> Result<bool, RuntimeError> {
        match self.peek()? {
            Some(b't') => {
                self.literal(b"true")?;
                Ok(true)
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(false)
            }
            _ => Err(RuntimeError::Protocol("JSON boolean")),
        }
    }
    pub fn null(&mut self) -> Result<bool, RuntimeError> {
        if self.peek()? != Some(b'n') {
            return Ok(false);
        }
        self.literal(b"null")?;
        Ok(true)
    }
    fn literal(&mut self, s: &[u8]) -> Result<(), RuntimeError> {
        for &b in s {
            self.expect(b)?;
        }
        Ok(())
    }
    pub fn integer(&mut self) -> Result<i64, RuntimeError> {
        let negative = self.peek()? == Some(b'-');
        if negative {
            self.byte()?;
        }
        let first = self.byte()?;
        if !first.is_ascii_digit() {
            return Err(RuntimeError::Protocol("JSON integer"));
        }
        let mut n = u64::from(first - b'0');
        while let Some(b'0'..=b'9') = self.peek()? {
            if first == b'0' {
                return Err(RuntimeError::Protocol("JSON leading zero"));
            }
            let digit = self.byte()? - b'0';
            n = n
                .checked_mul(10)
                .and_then(|n| n.checked_add(u64::from(digit)))
                .ok_or(RuntimeError::Protocol("JSON integer overflow"))?;
        }
        if matches!(self.peek()?, Some(b'.' | b'e' | b'E')) {
            return Err(RuntimeError::Protocol("selected integer type"));
        }
        if negative {
            if n == 1u64 << 63 {
                Ok(i64::MIN)
            } else {
                Ok(-i64::try_from(n).map_err(|_| RuntimeError::Protocol("JSON integer width"))?)
            }
        } else {
            i64::try_from(n).map_err(|_| RuntimeError::Protocol("JSON integer width"))
        }
    }
    pub fn skip(&mut self, depth: u8) -> Result<(), RuntimeError> {
        if depth > 32 {
            return Err(RuntimeError::Protocol("Engine metadata schema depth"));
        }
        self.whitespace()?;
        match self.peek()? {
            Some(b'"') => {
                self.string(false)?;
            }
            Some(b'{') => self.object(|r, _| r.skip(depth + 1))?,
            Some(b'[') => {
                self.byte()?;
                self.whitespace()?;
                if self.peek()? == Some(b']') {
                    self.byte()?;
                } else {
                    loop {
                        self.skip(depth + 1)?;
                        self.whitespace()?;
                        match self.byte()? {
                            b']' => break,
                            b',' => (),
                            _ => return Err(RuntimeError::Protocol("JSON array separator")),
                        }
                    }
                }
            }
            Some(b't') => self.literal(b"true")?,
            Some(b'f') => self.literal(b"false")?,
            Some(b'n') => self.literal(b"null")?,
            Some(b'-' | b'0'..=b'9') => self.skip_number()?,
            _ => return Err(RuntimeError::Protocol("JSON value")),
        }
        Ok(())
    }
    fn digits(&mut self) -> Result<(), RuntimeError> {
        if !matches!(self.peek()?, Some(b'0'..=b'9')) {
            return Err(RuntimeError::Protocol("JSON number digits"));
        }
        while matches!(self.peek()?, Some(b'0'..=b'9')) {
            self.byte()?;
        }
        Ok(())
    }
    fn skip_number(&mut self) -> Result<(), RuntimeError> {
        if self.peek()? == Some(b'-') {
            self.byte()?;
        }
        if self.peek()? == Some(b'0') {
            self.byte()?;
            if matches!(self.peek()?, Some(b'0'..=b'9')) {
                return Err(RuntimeError::Protocol("JSON leading zero"));
            }
        } else {
            self.digits()?;
        }
        if self.peek()? == Some(b'.') {
            self.byte()?;
            self.digits()?;
        }
        if matches!(self.peek()?, Some(b'e' | b'E')) {
            self.byte()?;
            if matches!(self.peek()?, Some(b'+' | b'-')) {
                self.byte()?;
            }
            self.digits()?;
        }
        Ok(())
    }
}
pub(super) fn once(bits: &mut u8, bit: u8) -> Result<(), RuntimeError> {
    if *bits & bit != 0 {
        return Err(RuntimeError::Protocol("duplicate deciding JSON field"));
    }
    *bits |= bit;
    Ok(())
}

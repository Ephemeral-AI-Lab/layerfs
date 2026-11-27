//! Shallow JSON reading (an image archive's manifest, platform facts), streamed
//! without collecting unknown values.
use super::strings;
use super::JsonError;
use std::io::Read;
/// A shallow streamed JSON reader over `R`, one byte of look-ahead.
pub struct Json<R> {
    input: R,
    peeked: Option<u8>,
}
impl<R: Read> Json<R> {
    /// A reader at the start of `input`.
    pub fn new(input: R) -> Self {
        Self {
            input,
            peeked: None,
        }
    }
    /// The next byte, not consumed; `None` at the end.
    pub fn peek(&mut self) -> Result<Option<u8>, JsonError> {
        if self.peeked.is_none() {
            let mut one = [0];
            if self.input.read(&mut one)? == 0 {
                return Ok(None);
            }
            self.peeked = Some(one[0]);
        }
        Ok(self.peeked)
    }
    /// The next byte, consumed; the end is refused.
    pub fn byte(&mut self) -> Result<u8, JsonError> {
        self.peek()?.ok_or(JsonError("truncated JSON".to_owned()))?;
        Ok(self.peeked.take().expect("peeked"))
    }
    /// Consumes byte `b`, refusing any other.
    pub fn expect(&mut self, b: u8) -> Result<(), JsonError> {
        if self.byte()? != b {
            return Err(JsonError("JSON punctuation".to_owned()));
        }
        Ok(())
    }
    /// Skips JSON whitespace.
    pub fn whitespace(&mut self) -> Result<(), JsonError> {
        while matches!(self.peek()?, Some(b' ' | b'\r' | b'\n' | b'\t')) {
            self.byte()?;
        }
        Ok(())
    }
    /// Refuses anything but whitespace before the end.
    pub fn finish(&mut self) -> Result<(), JsonError> {
        self.whitespace()?;
        if self.peek()?.is_some() {
            return Err(JsonError("trailing JSON value".to_owned()));
        }
        Ok(())
    }
    /// Reads one object, calling `field` with each key; it must read the value.
    /// `field` may fail with any error a [`JsonError`] converts into.
    pub fn object<E: From<JsonError>>(
        &mut self,
        mut field: impl FnMut(&mut Self, &strings::Text) -> Result<(), E>,
    ) -> Result<(), E> {
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
                _ => return Err(JsonError("JSON object separator".to_owned()).into()),
            }
        }
    }
    /// Reads one array, calling `element` for each element; it must read it.
    /// `element` may fail with any error a [`JsonError`] converts into.
    pub fn array<E: From<JsonError>>(
        &mut self,
        mut element: impl FnMut(&mut Self) -> Result<(), E>,
    ) -> Result<(), E> {
        self.whitespace()?;
        self.expect(b'[')?;
        self.whitespace()?;
        if self.peek()? == Some(b']') {
            self.byte()?;
            return Ok(());
        }
        loop {
            element(self)?;
            self.whitespace()?;
            match self.byte()? {
                b']' => return Ok(()),
                b',' => self.whitespace()?,
                _ => return Err(JsonError("JSON array separator".to_owned()).into()),
            }
        }
    }
    /// Reads `true` or `false`.
    pub fn boolean(&mut self) -> Result<bool, JsonError> {
        match self.peek()? {
            Some(b't') => {
                self.literal(b"true")?;
                Ok(true)
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(false)
            }
            _ => Err(JsonError("JSON boolean".to_owned())),
        }
    }
    /// Reads `null` when it is next: whether it was.
    pub fn null(&mut self) -> Result<bool, JsonError> {
        if self.peek()? != Some(b'n') {
            return Ok(false);
        }
        self.literal(b"null")?;
        Ok(true)
    }
    fn literal(&mut self, s: &[u8]) -> Result<(), JsonError> {
        for &b in s {
            self.expect(b)?;
        }
        Ok(())
    }
    /// Reads a whole number that fits an `i64`.
    pub fn integer(&mut self) -> Result<i64, JsonError> {
        let negative = self.peek()? == Some(b'-');
        if negative {
            self.byte()?;
        }
        let first = self.byte()?;
        if !first.is_ascii_digit() {
            return Err(JsonError("JSON integer".to_owned()));
        }
        let mut n = u64::from(first - b'0');
        while let Some(b'0'..=b'9') = self.peek()? {
            if first == b'0' {
                return Err(JsonError("JSON leading zero".to_owned()));
            }
            let digit = self.byte()? - b'0';
            n = n
                .checked_mul(10)
                .and_then(|n| n.checked_add(u64::from(digit)))
                .ok_or(JsonError("JSON integer overflow".to_owned()))?;
        }
        if matches!(self.peek()?, Some(b'.' | b'e' | b'E')) {
            return Err(JsonError("selected integer type".to_owned()));
        }
        if negative {
            if n == 1u64 << 63 {
                Ok(i64::MIN)
            } else {
                Ok(-i64::try_from(n).map_err(|_| JsonError("JSON integer width".to_owned()))?)
            }
        } else {
            i64::try_from(n).map_err(|_| JsonError("JSON integer width".to_owned()))
        }
    }
    /// Skips one value nested at most to `depth`.
    pub fn skip(&mut self, depth: u8) -> Result<(), JsonError> {
        if depth > 32 {
            return Err(JsonError("Engine metadata schema depth".to_owned()));
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
                            _ => return Err(JsonError("JSON array separator".to_owned()).into()),
                        }
                    }
                }
            }
            Some(b't') => self.literal(b"true")?,
            Some(b'f') => self.literal(b"false")?,
            Some(b'n') => self.literal(b"null")?,
            Some(b'-' | b'0'..=b'9') => self.skip_number()?,
            _ => return Err(JsonError("JSON value".to_owned())),
        }
        Ok(())
    }
    fn digits(&mut self) -> Result<(), JsonError> {
        if !matches!(self.peek()?, Some(b'0'..=b'9')) {
            return Err(JsonError("JSON number digits".to_owned()));
        }
        while matches!(self.peek()?, Some(b'0'..=b'9')) {
            self.byte()?;
        }
        Ok(())
    }
    fn skip_number(&mut self) -> Result<(), JsonError> {
        if self.peek()? == Some(b'-') {
            self.byte()?;
        }
        if self.peek()? == Some(b'0') {
            self.byte()?;
            if matches!(self.peek()?, Some(b'0'..=b'9')) {
                return Err(JsonError("JSON leading zero".to_owned()));
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

/// Marks deciding field `bit` seen in `bits`, refusing a field seen twice
/// (the namespaces backend's image archive reader, Linux only).
pub fn once(bits: &mut u8, bit: u8) -> Result<(), JsonError> {
    if *bits & bit != 0 {
        return Err(JsonError("duplicate deciding JSON field".to_owned()));
    }
    *bits |= bit;
    Ok(())
}

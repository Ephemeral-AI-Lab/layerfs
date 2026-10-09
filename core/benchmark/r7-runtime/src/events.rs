use crate::Result;
use std::fmt::Write as FmtWrite;
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Instant,
};

/// Append-only newline JSON. Debug records are opaque diagnostics, never parsed as outcomes.
pub struct Events {
    file: File,
    pub origin: Instant,
    pub directory: PathBuf,
    sequence: u64,
}
#[derive(Clone, Copy)]
pub struct Span {
    pub start_ns: u128,
    pub end_ns: u128,
    pub duration_ns: u128,
}
pub fn quoted(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
impl Events {
    pub fn new(path: &Path) -> Result<Self> {
        path.parent().ok_or("receipt parent")?;
        let directory = path.with_extension("artifacts");
        std::fs::create_dir(&directory)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        Ok(Self {
            file,
            origin: Instant::now(),
            directory,
            sequence: 0,
        })
    }
    pub fn value(&mut self, event: &str, value: &str) -> Result<()> {
        self.phase(event, None, value)
    }
    pub fn span(&self, start: Instant) -> Span {
        let end = Instant::now();
        Span {
            start_ns: start.duration_since(self.origin).as_nanos(),
            end_ns: end.duration_since(self.origin).as_nanos(),
            duration_ns: end.duration_since(start).as_nanos(),
        }
    }
    pub fn phase(&mut self, event: &str, elapsed: Option<Span>, value: &str) -> Result<()> {
        self.fields(event, elapsed, value, &[])
    }
    pub fn fields(
        &mut self,
        event: &str,
        elapsed: Option<Span>,
        value: &str,
        fields: &[(&str, String)],
    ) -> Result<()> {
        self.numeric_fields(event, elapsed, value, fields, &[])
    }
    pub fn numeric_fields(
        &mut self,
        event: &str,
        elapsed: Option<Span>,
        value: &str,
        fields: &[(&str, String)],
        numbers: &[(&str, u64)],
    ) -> Result<()> {
        self.sequence += 1;
        let span = elapsed.map(|span|format!("{{\"clock_id\":\"monotonic\",\"start_ns\":{},\"end_ns\":{},\"duration_ns\":{}}}", span.start_ns,span.end_ns,span.duration_ns)).unwrap_or_else(||"null".into());
        let mut fields = fields
            .iter()
            .map(|(key, value)| format!("{}:{}", quoted(key), quoted(value)))
            .collect::<Vec<_>>()
            .join(",");
        for (key, value) in numbers {
            if !fields.is_empty() {
                fields.push(',');
            }
            write!(&mut fields, "{}:{value}", quoted(key))?;
        }
        let record = format!("{{\"schema\":\"r7-runtime-events-v1\",\"sequence\":{},\"at_ns\":{},\"event\":{},\"elapsed_ns\":{},\"span\":{},\"value\":{},\"fields\":{{{fields}}}}}\n", self.sequence, self.origin.elapsed().as_nanos(), quoted(event), elapsed.map(|v|v.duration_ns.to_string()).unwrap_or_else(||"null".into()), span, quoted(value));
        self.file.write_all(record.as_bytes())?;
        self.file.flush()?;
        let mut stdout = io::stdout().lock();
        stdout.write_all(record.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }
    pub fn output(&mut self, label: &str, stream: &str) -> Result<(PathBuf, File)> {
        let path = self
            .directory
            .join(format!("{:06}-{label}.{stream}", self.sequence + 1));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        Ok((path, file))
    }
}

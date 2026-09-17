//! Fixed-format JSON output for a completed timing tree.
//!
//! This is a small writer for one documented shape, not a general serializer:
//! it emits the report through a caller-supplied [`Write`] and propagates every
//! writer error.

use std::io::{self, Write};
use std::time::Duration;

use crate::timer::report::{TimingNode, TimingReport};

impl TimingReport {
    /// Writes the completed report as JSON through `writer`.
    ///
    /// A measured report is one object with `name`, `elapsed_ns` and `children`.
    /// The `outcome` and `incomplete` fields appear only when they are not the
    /// default. A disabled report writes `null`.
    pub fn write_json(&self, mut writer: impl Write) -> io::Result<()> {
        match self.root() {
            None => writer.write_all(b"null\n"),
            Some(root) => {
                write_node(&mut writer, root, 0)?;
                writer.write_all(b"\n")
            }
        }
    }
}

fn write_node(writer: &mut impl Write, node: &TimingNode, depth: usize) -> io::Result<()> {
    if node.children().is_empty() {
        return write_leaf(writer, node);
    }
    writer.write_all(b"{\n")?;
    write_indent(writer, depth + 1)?;
    write_name_field(writer, node.name())?;
    writer.write_all(b",\n")?;
    write_indent(writer, depth + 1)?;
    write_elapsed_field(writer, node.elapsed())?;
    writer.write_all(b",\n")?;
    write_flag_fields(writer, node, depth + 1)?;
    write_indent(writer, depth + 1)?;
    writer.write_all(b"\"children\": [\n")?;
    let children = node.children();
    for (index, child) in children.iter().enumerate() {
        write_indent(writer, depth + 2)?;
        write_node(writer, child, depth + 2)?;
        let separator: &[u8] = if index + 1 == children.len() {
            b"\n"
        } else {
            b",\n"
        };
        writer.write_all(separator)?;
    }
    write_indent(writer, depth + 1)?;
    writer.write_all(b"]\n")?;
    write_indent(writer, depth)?;
    writer.write_all(b"}")
}

fn write_leaf(writer: &mut impl Write, node: &TimingNode) -> io::Result<()> {
    writer.write_all(b"{")?;
    write_name_field(writer, node.name())?;
    writer.write_all(b", ")?;
    write_elapsed_field(writer, node.elapsed())?;
    write_inline_flags(writer, node)?;
    writer.write_all(b", \"children\": []}")
}

fn write_flag_fields(writer: &mut impl Write, node: &TimingNode, depth: usize) -> io::Result<()> {
    if !matches!(node.outcome(), crate::timer::report::NodeOutcome::Ok) {
        write_indent(writer, depth)?;
        writeln!(writer, "\"outcome\": \"{}\",", node.outcome().as_str())?;
    }
    if node.is_incomplete() {
        write_indent(writer, depth)?;
        writer.write_all(b"\"incomplete\": true,\n")?;
    }
    Ok(())
}

fn write_inline_flags(writer: &mut impl Write, node: &TimingNode) -> io::Result<()> {
    if !matches!(node.outcome(), crate::timer::report::NodeOutcome::Ok) {
        write!(writer, ", \"outcome\": \"{}\"", node.outcome().as_str())?;
    }
    if node.is_incomplete() {
        writer.write_all(b", \"incomplete\": true")?;
    }
    Ok(())
}

fn write_name_field(writer: &mut impl Write, name: &str) -> io::Result<()> {
    writer.write_all(b"\"name\": \"")?;
    write_escaped(writer, name)?;
    writer.write_all(b"\"")
}

fn write_elapsed_field(writer: &mut impl Write, elapsed: Duration) -> io::Result<()> {
    let nanos = u64::try_from(elapsed.as_nanos()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "elapsed duration exceeds the u64 nanosecond range",
        )
    })?;
    write!(writer, "\"elapsed_ns\": {nanos}")
}

fn write_escaped(writer: &mut impl Write, text: &str) -> io::Result<()> {
    let mut plain_start = 0;
    for (index, character) in text.char_indices() {
        let escape = match character {
            '"' => Some("\\\""),
            '\\' => Some("\\\\"),
            '\u{08}' => Some("\\b"),
            '\u{0c}' => Some("\\f"),
            '\n' => Some("\\n"),
            '\r' => Some("\\r"),
            '\t' => Some("\\t"),
            value if value < ' ' => None,
            _ => continue,
        };
        writer.write_all(&text.as_bytes()[plain_start..index])?;
        match escape {
            Some(text) => writer.write_all(text.as_bytes())?,
            None => write!(writer, "\\u{:04x}", character as u32)?,
        }
        plain_start = index + character.len_utf8();
    }
    writer.write_all(&text.as_bytes()[plain_start..])
}

fn write_indent(writer: &mut impl Write, depth: usize) -> io::Result<()> {
    for _ in 0..depth {
        writer.write_all(b"  ")?;
    }
    Ok(())
}

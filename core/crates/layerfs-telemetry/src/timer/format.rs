//! Readable presentation of a completed timing tree.

use std::io::{self, Write};
use std::time::Duration;

use crate::timer::report::{TimingNode, TimingReport};

impl TimingReport {
    /// Writes the completed report as a readable, indented tree.
    ///
    /// Labels are escaped so that control characters cannot break the layout,
    /// elapsed times use a readable unit, and error or missing detail is marked
    /// explicitly. A disabled report writes one `disabled` line.
    pub fn write_text(&self, mut writer: impl Write) -> io::Result<()> {
        let Some(root) = self.root() else {
            return writer.write_all(b"disabled: no timing recorded\n");
        };
        write_node(&mut writer, root, 0)
    }
}

fn write_node(writer: &mut impl Write, node: &TimingNode, depth: usize) -> io::Result<()> {
    write_indent(writer, depth)?;
    write_escaped(writer, node.name())?;
    write!(writer, "  {}", elapsed_text(node.elapsed()))?;
    match node.outcome() {
        crate::timer::report::NodeOutcome::Ok => {}
        crate::timer::report::NodeOutcome::Error => writer.write_all(b" [error]")?,
        crate::timer::report::NodeOutcome::Unknown => writer.write_all(b" [unknown]")?,
    }
    if node.is_incomplete() {
        writer.write_all(b" [incomplete]")?;
    }
    writer.write_all(b"\n")?;
    for child in node.children() {
        write_node(writer, child, depth + 1)?;
    }
    Ok(())
}

fn write_indent(writer: &mut impl Write, depth: usize) -> io::Result<()> {
    for _ in 0..depth {
        writer.write_all(b"  ")?;
    }
    Ok(())
}

fn write_escaped(writer: &mut impl Write, label: &str) -> io::Result<()> {
    let mut plain_start = 0;
    for (index, character) in label.char_indices() {
        let escape = match character {
            '\\' => Some("\\\\"),
            '\n' => Some("\\n"),
            '\r' => Some("\\r"),
            '\t' => Some("\\t"),
            value if value.is_control() => None,
            _ => continue,
        };
        writer.write_all(&label.as_bytes()[plain_start..index])?;
        match escape {
            Some(text) => writer.write_all(text.as_bytes())?,
            None => write!(writer, "\\u{{{:x}}}", character as u32)?,
        }
        plain_start = index + character.len_utf8();
    }
    writer.write_all(&label.as_bytes()[plain_start..])
}

fn elapsed_text(elapsed: Duration) -> String {
    let nanos = elapsed.as_nanos();
    if nanos < 1_000 {
        format!("{nanos}ns")
    } else if nanos < 1_000_000 {
        format!("{}.{:03}us", nanos / 1_000, nanos % 1_000)
    } else if nanos < 1_000_000_000 {
        format!("{}.{:03}ms", nanos / 1_000_000, (nanos / 1_000) % 1_000)
    } else {
        format!(
            "{}.{:03}s",
            nanos / 1_000_000_000,
            (nanos / 1_000_000) % 1_000
        )
    }
}

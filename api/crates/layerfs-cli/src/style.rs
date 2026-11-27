//! Terminal styling: bold headings and coloured diff lines, only when
//! stdout is an interactive terminal (TTY) and `NO_COLOR` is unset
//! (https://no-color.org/). Machine output never carries it. Raw ANSI
//! escapes; no color dependency.
use std::io::IsTerminal;

/// `text` in the ANSI style `code` on an interactive terminal without
/// `NO_COLOR`; as it is otherwise.
pub fn paint(text: &str, code: &str) -> String {
    match std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        true => format!("\x1b[{code}m{text}\x1b[0m"),
        false => text.to_owned(),
    }
}

/// `text` in bold on an interactive terminal; as it is otherwise.
pub fn bold(text: &str) -> String {
    paint(text, "1")
}

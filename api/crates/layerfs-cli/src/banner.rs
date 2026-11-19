//! The startup banner, shown only on an interactive terminal (TTY): brand
//! and purpose. Machine output (JSON results, help piped into another
//! program) never carries it. Colors are raw ANSI escapes; no color
//! dependency.
use std::io::IsTerminal;

// Brand blue: bright blue foreground; reset afterwards. NO_COLOR turns
// every color off (https://no-color.org/).
pub fn paint(text: &str, code: &str) -> String {
    if std::env::var_os("NO_COLOR").is_some() {
        text.to_owned()
    } else {
        format!("\x1b[{code}m{text}\x1b[0m")
    }
}

/// `text` in bold on an interactive terminal; as it is otherwise.
pub fn bold(text: &str) -> String {
    match std::io::stdout().is_terminal() {
        true => paint(text, "1"),
        false => text.to_owned(),
    }
}

fn blue(text: &str) -> String {
    paint(text, "34;1")
}

fn dim(text: &str) -> String {
    paint(text, "2")
}

const LOGO_LINES: [&str; 6] = [
    "  _                       _____ ____  ",
    " | |   __ _ _   _  ___ _ _|  ___/ ___| ",
    " | |  / _` | | | |/ _ \\ '__| |_ \\___ \\ ",
    " | |_| (_| | |_| |  __/ |  |  _| ___) |",
    " |____\\__,_|\\__, |\\___|_|  |_|  |____/ ",
    "            |___/                     ",
];

fn logo() -> String {
    LOGO_LINES.map(blue).join("\n")
}

fn tagline() -> String {
    dim("a versioned filesystem for AI agents: commit, branch and roll back every file an agent touches")
}

/// The banner text (logo, tagline).
pub fn render() -> String {
    format!("{}\n\n{}\n\n", logo(), tagline())
}

/// The banner text, or nothing when stdout is not an interactive terminal.
pub fn banner() -> String {
    if !std::io::stdout().is_terminal() {
        return String::new();
    }
    render()
}

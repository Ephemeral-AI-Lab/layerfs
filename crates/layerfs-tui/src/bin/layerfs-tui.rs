use std::{env, io, io::IsTerminal};

fn main() -> io::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|value| value == "--dump") {
        let page = args.get(1).map(String::as_str).unwrap_or("projects");
        let width = flag(&args, "--width").unwrap_or(120);
        let height = flag(&args, "--height").unwrap_or(40);
        let no_color = args.iter().any(|value| value == "--no-color");
        print!(
            "{}",
            layerfs_tui::render_to_string(page, width, height, no_color)?
        );
        return Ok(());
    }

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "interactive TUI requires a terminal; use --dump <page>",
        ));
    }
    let mut terminal = ratatui::init();
    let result = match string_flag(&args, "--context") {
        Some(context) => layerfs_tui::run_with_context(&mut terminal, context),
        None => layerfs_tui::run(&mut terminal),
    };
    ratatui::restore();
    result
}

fn string_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|value| value == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn flag(args: &[String], name: &str) -> Option<u16> {
    args.iter()
        .position(|value| value == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
}

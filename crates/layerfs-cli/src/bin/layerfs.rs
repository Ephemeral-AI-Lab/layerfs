fn main() {
    let mut arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() == 1 && arguments[0] == "--help" {
        print!("{}", layerfs_cli::HELP);
        return;
    }
    if arguments.len() == 1 && arguments[0] == "--version" {
        println!("layerfs {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let json = arguments.first().is_some_and(|value| value == "--json");
    if json {
        arguments.remove(0);
    }
    #[cfg(unix)]
    let result = if arguments.first().and_then(|value| value.to_str())
        == Some("__layerfs_context_owner")
        && arguments.len() == 2
    {
        layerfs_cli::serve_context_owner(arguments.remove(1).into()).map(|()| 0)
    } else {
        layerfs_cli::invoke_managed(
            layerfs_cli::default_context_location(),
            arguments,
            json,
            &mut std::io::stdout(),
        )
    };
    #[cfg(not(unix))]
    let result = layerfs_cli::invoke_managed(
        layerfs_cli::default_context_location(),
        arguments,
        json,
        &mut std::io::stdout(),
    );
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

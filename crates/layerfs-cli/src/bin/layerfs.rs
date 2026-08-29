fn main() {
    let mut arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = if arguments
        .first()
        .is_some_and(|argument| argument == "--host")
    {
        if arguments.len() != 2 {
            Err(layerfs_cli::CliError::Invalid("host arguments".to_owned()))
        } else {
            layerfs_cli::serve(&arguments[1])
        }
        .map(|()| 0)
    } else {
        layerfs_cli::invoke(
            layerfs_cli::default_context_location(),
            std::mem::take(&mut arguments),
            false,
            &mut std::io::stdout(),
        )
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

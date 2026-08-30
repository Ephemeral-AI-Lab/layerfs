fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = layerfs_cli::invoke(
        layerfs_cli::default_context_location(),
        arguments,
        false,
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

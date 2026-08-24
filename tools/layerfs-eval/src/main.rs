mod apple_poc;

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let result = match (arguments.next(), arguments.next(), arguments.next()) {
        (Some(command), Some(directory), None) if command == "apple-poc" => {
            apple_poc::run(std::path::Path::new(&directory))
        }
        _ => Err("usage: layerfs-eval apple-poc <run-directory>".to_owned()),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

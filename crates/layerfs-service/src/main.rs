fn main() {
    if let Err(error) = layerfs_service::cli::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

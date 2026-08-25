mod apple_poc;
mod stage1;
mod stage1_edge;
mod stage1_fixture;

fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = match arguments.as_slice() {
        [command, directory] if command == "apple-poc" => {
            apple_poc::run(std::path::Path::new(directory))
        }
        [stage1, prepare, single]
            if stage1 == "stage1" && prepare == "prepare" && single == "single-file" =>
        {
            stage1_fixture::prepare_single_file()
        }
        [stage1, prepare, edge]
            if stage1 == "stage1" && prepare == "prepare" && edge == "apple-edge" =>
        {
            stage1_edge::prepare()
        }
        [stage1, readiness, single]
            if stage1 == "stage1"
                && (readiness == "readiness-only" || readiness == "readiness")
                && single == "single-file" =>
        {
            stage1::readiness_single_file()
        }
        [stage1, readiness, edge]
            if stage1 == "stage1" && readiness == "readiness" && edge == "apple-edge" =>
        {
            stage1_edge::readiness()
        }
        [stage1, run, single, directory]
            if stage1 == "stage1" && run == "run" && single == "single-file" =>
        {
            stage1::run_single_file(std::path::Path::new(directory))
        }
        [stage1, run, edge, directory]
            if stage1 == "stage1" && run == "run" && edge == "apple-edge" =>
        {
            stage1_edge::run(std::path::Path::new(directory))
        }
        _ => Err(
            "usage:\n  layerfs-eval apple-poc <run-directory>\n  layerfs-eval stage1 prepare single-file\n  layerfs-eval stage1 readiness-only single-file\n  layerfs-eval stage1 run single-file <run-directory>\n  layerfs-eval stage1 prepare apple-edge\n  layerfs-eval stage1 readiness apple-edge\n  layerfs-eval stage1 run apple-edge <run-directory>"
                .to_owned(),
        ),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

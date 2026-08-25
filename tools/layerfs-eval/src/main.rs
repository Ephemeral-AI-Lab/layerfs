mod apple_poc;
mod stage1;
mod stage1_edge;
mod stage1_fixture;
mod stage1_materialize;

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
        [stage1, materialize, parity_row, store, source, size_mib, work, identity]
            if stage1 == "stage1"
                && materialize == "materialize"
                && parity_row == "parity-row" =>
        {
            stage1_materialize::parity_row(
                std::path::Path::new(store),
                std::path::Path::new(source),
                size_mib,
                std::path::Path::new(work),
                identity,
            )
        }
        [stage1, materialize, attribution_block, store, source, size_mib, arm, work, identity]
            if stage1 == "stage1"
                && materialize == "materialize"
                && attribution_block == "attribution-block" =>
        {
            stage1_materialize::attribution_block(
                std::path::Path::new(store),
                std::path::Path::new(source),
                size_mib,
                arm,
                std::path::Path::new(work),
                identity,
            )
        }
        [stage1, materialize, trusted_block, store, source, size_mib, work, identity]
            if stage1 == "stage1"
                && materialize == "materialize"
                && trusted_block == "trusted-block" =>
        {
            stage1_materialize::trusted_block(
                std::path::Path::new(store),
                std::path::Path::new(source),
                size_mib,
                std::path::Path::new(work),
                identity,
            )
        }
        [stage1, materialize, hash, path]
            if stage1 == "stage1" && materialize == "materialize" && hash == "hash" =>
        {
            stage1_materialize::hash(std::path::Path::new(path))
        }
        [stage1, materialize, prepare]
            if stage1 == "stage1" && materialize == "materialize" && prepare == "prepare" =>
        {
            stage1_materialize::prepare()
        }
        [stage1, materialize, manifest, role, commit, executable, build_target, build_log, output]
            if stage1 == "stage1" && materialize == "materialize" && manifest == "manifest" =>
        {
            stage1_materialize::manifest(
                role,
                commit,
                std::path::Path::new(executable),
                std::path::Path::new(build_target),
                std::path::Path::new(build_log),
                std::path::Path::new(output),
            )
        }
        [stage1, materialize, parity_readiness, historical, instrumented, store, source, receipt]
            if stage1 == "stage1"
                && materialize == "materialize"
                && parity_readiness == "parity-readiness" =>
        {
            stage1_materialize::parity_readiness(
                std::path::Path::new(historical),
                std::path::Path::new(instrumented),
                std::path::Path::new(store),
                std::path::Path::new(source),
                std::path::Path::new(receipt),
            )
        }
        [stage1, materialize, parity_run, historical, instrumented, store, source, readiness, run]
            if stage1 == "stage1"
                && materialize == "materialize"
                && parity_run == "parity-run" =>
        {
            stage1_materialize::parity_run(
                std::path::Path::new(historical),
                std::path::Path::new(instrumented),
                std::path::Path::new(store),
                std::path::Path::new(source),
                std::path::Path::new(readiness),
                std::path::Path::new(run),
            )
        }
        [stage1, materialize, attribution_run, control, fixture, run]
            if stage1 == "stage1"
                && materialize == "materialize"
                && attribution_run == "attribution-run" =>
        {
            stage1_materialize::attribution_run(
                std::path::Path::new(control),
                std::path::Path::new(fixture),
                std::path::Path::new(run),
            )
        }
        [stage1, materialize, trusted_run, fixture, source_manifest, run]
            if stage1 == "stage1"
                && materialize == "materialize"
                && trusted_run == "trusted-run" =>
        {
            stage1_materialize::trusted_run(
                std::path::Path::new(fixture),
                std::path::Path::new(source_manifest),
                std::path::Path::new(run),
            )
        }
        [stage1, materialize, acceptance_run, control, candidate, fixture, run]
            if stage1 == "stage1"
                && materialize == "materialize"
                && acceptance_run == "acceptance-run" =>
        {
            stage1_materialize::acceptance_run(
                std::path::Path::new(control),
                std::path::Path::new(candidate),
                std::path::Path::new(fixture),
                std::path::Path::new(run),
            )
        }
        _ => Err(
            "usage:\n  layerfs-eval apple-poc <run-directory>\n  layerfs-eval stage1 prepare single-file\n  layerfs-eval stage1 readiness-only single-file\n  layerfs-eval stage1 run single-file <run-directory>\n  layerfs-eval stage1 prepare apple-edge\n  layerfs-eval stage1 readiness apple-edge\n  layerfs-eval stage1 run apple-edge <run-directory>\n  layerfs-eval stage1 materialize prepare\n  layerfs-eval stage1 materialize parity-row <store> <source> <size-mib> <work-directory> <identity>\n  layerfs-eval stage1 materialize attribution-block <store> <source> <size-mib> <complete|null|digest|native> <work-directory> <identity>\n  layerfs-eval stage1 materialize trusted-block <store> <source> <size-mib> <work-directory> <identity>\n  layerfs-eval stage1 materialize trusted-run <fixture-root> <source-manifest.json> <new-run-directory>\n  layerfs-eval stage1 materialize attribution-run <control-executable> <fixture-root> <new-run-directory>\n  layerfs-eval stage1 materialize acceptance-run <control-executable> <candidate-executable> <fixture-root> <new-run-directory>\n  layerfs-eval stage1 materialize hash <path>\n  layerfs-eval stage1 materialize manifest <role> <commit> <executable> <build-target-dir> <build.log> <output.json>\n  layerfs-eval stage1 materialize parity-readiness <historical> <instrumented> <store> <source> <receipt.json>\n  layerfs-eval stage1 materialize parity-run <historical> <instrumented> <store> <source> <readiness.json> <new-run-directory>"
                .to_owned(),
        ),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

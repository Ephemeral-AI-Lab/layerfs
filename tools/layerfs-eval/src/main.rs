use blake3::Hasher;
use layerfs_core::{
    decode_object, decode_object_from, encode_object, encode_object_to, CanonicalName,
    CanonicalPath, DirectoryEntry, Object, ObjectId, ObjectKind, ObjectReference,
};
use layerfs_os::{probe, HostEnvironment};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const FORMAT_VERSION: u32 = 1;
const SEED: u64 = 0x4c41594552534653;
const BUFFER_SIZE: usize = 1024 * 1024;
const SINGLE_FILES: &[(&str, u64)] = &[
    ("S1-16", 16 * 1024 * 1024),
    ("S1-100", 100 * 1024 * 1024),
    ("S1-512", 512 * 1024 * 1024),
];
const TREE_FILE_COUNT: usize = 10_000;

type EvalResult<T> = Result<T, String>;

#[derive(Debug, Clone)]
struct FileManifest {
    path: String,
    size: u64,
    blake3: String,
}

#[derive(Debug, Clone)]
struct DatasetManifest {
    id: String,
    files: Vec<FileManifest>,
    empty_dirs: Vec<String>,
    root_input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeEntry {
    kind: String,
    size: u64,
    blake3: Option<String>,
    target: Option<String>,
}

#[derive(Debug)]
struct Mismatch {
    path: String,
    issue: String,
    expected: Option<TreeEntry>,
    actual: Option<TreeEntry>,
}

fn main() {
    let result = match env::args().nth(1).as_deref() {
        Some("b0") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval b0 <run-directory>".to_owned())
            .and_then(|path| run_b0(Path::new(&path)).map(|_| 0)),
        Some("dataset") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval dataset <dataset-directory>".to_owned())
            .and_then(|path| {
                let output = Path::new(&path);
                prepare_empty_directory(output)?;
                let manifest = generate_dataset_set(output)?;
                write_text(&output.join("dataset.json"), &dataset_set_json(&manifest))?;
                Ok(0)
            }),
        Some("probe") => {
            let directory = env::args().nth(2);
            let output = env::args().nth(3);
            match (directory, output) {
                (Some(directory), Some(output)) => {
                    write_probe(Path::new(&directory), Path::new(&output))
                }
                _ => Err("usage: layerfs-eval probe <directory> <output-json>".to_owned()),
            }
        }
        Some("phase1") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval phase1 <run-directory>".to_owned())
            .and_then(|path| run_phase1(Path::new(&path)).map(|_| 0)),
        Some("oracle") => {
            let expected = env::args().nth(2);
            let actual = env::args().nth(3);
            let output = env::args().nth(4);
            match (expected, actual, output) {
                (Some(expected), Some(actual), Some(output)) => {
                    run_oracle(Path::new(&expected), Path::new(&actual), Path::new(&output))
                }
                _ => Err(
                    "usage: layerfs-eval oracle <expected-directory> <actual-directory> <output-json>"
                        .to_owned(),
                ),
            }
        }
        Some("help") | None => {
            print_help();
            Ok(0)
        }
        Some(command) => Err(format!("unknown command: {command}")),
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!("layerfs-eval b0 <run-directory>");
    println!("layerfs-eval dataset <dataset-directory>");
    println!("layerfs-eval probe <directory> <output-json>");
    println!("layerfs-eval phase1 <run-directory>");
    println!("layerfs-eval oracle <expected-directory> <actual-directory> <output-json>");
}

const PHASE1_WARMUPS: usize = 1;
const PHASE1_ITERATIONS: usize = 5;
const PHASE1_BYTE_SIZES: &[usize] = &[1024, 1024 * 1024, 8 * 1024 * 1024];
const PHASE1_DIRECTORY_FANOUTS: &[usize] = &[16, 256, 4096];

#[derive(Debug)]
struct Phase1Run {
    case: String,
    input_bytes: usize,
    output_bytes: usize,
    iterations: usize,
    elapsed_ns: Vec<u128>,
    correct: bool,
}

fn run_phase1(run_directory: &Path) -> EvalResult<()> {
    prepare_phase1_directory(run_directory)?;
    let environment = host_environment(run_directory)?;
    write_text(
        &run_directory.join("environment.json"),
        &environment_json(&environment),
    )?;

    let mut runs = Vec::new();
    for &size in PHASE1_BYTE_SIZES {
        let object = phase1_bytes_object(size)?;
        let encoded =
            encode_object(&object).map_err(|error| format!("encode fixture: {error:?}"))?;
        let expected_id = ObjectId::for_bytes(&encoded);
        let label = format!("bytes-{size}");

        runs.push(measure_phase1(
            format!("{label}/encode_vec"),
            encoded.len(),
            encoded.len(),
            || {
                encode_object(&object)
                    .is_ok_and(|value| std::hint::black_box(value.len()) == encoded.len())
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/encode_writer"),
            encoded.len(),
            encoded.len(),
            || {
                let mut output = Vec::with_capacity(encoded.len());
                encode_object_to(&object, &mut output)
                    .is_ok_and(|_| std::hint::black_box(output.as_slice()) == encoded.as_slice())
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/decode_slice"),
            encoded.len(),
            size,
            || decode_object(&encoded).is_ok_and(|value| value == object),
        )?);
        runs.push(measure_phase1(
            format!("{label}/decode_reader"),
            encoded.len(),
            size,
            || {
                decode_object_from(Cursor::new(encoded.as_slice()))
                    .is_ok_and(|value| value == object)
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/hash_slice"),
            encoded.len(),
            32,
            || std::hint::black_box(ObjectId::for_bytes(&encoded)) == expected_id,
        )?);
        runs.push(measure_phase1(
            format!("{label}/hash_reader"),
            encoded.len(),
            32,
            || {
                ObjectId::from_reader(Cursor::new(encoded.as_slice()))
                    .is_ok_and(|value| value == expected_id)
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/object_id"),
            size,
            32,
            || object.id().is_ok_and(|value| value == expected_id),
        )?);
    }

    for &fanout in PHASE1_DIRECTORY_FANOUTS {
        let object = phase1_directory_object(fanout)?;
        let encoded =
            encode_object(&object).map_err(|error| format!("encode fixture: {error:?}"))?;
        let expected_id = ObjectId::for_bytes(&encoded);
        let label = format!("directory-{fanout}");
        runs.push(measure_phase1(
            format!("{label}/encode_vec"),
            encoded.len(),
            encoded.len(),
            || {
                encode_object(&object)
                    .is_ok_and(|value| std::hint::black_box(value.len()) == encoded.len())
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/decode_reader"),
            encoded.len(),
            encoded.len(),
            || {
                decode_object_from(Cursor::new(encoded.as_slice()))
                    .is_ok_and(|value| value == object)
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/hash_reader"),
            encoded.len(),
            32,
            || {
                ObjectId::from_reader(Cursor::new(encoded.as_slice()))
                    .is_ok_and(|value| value == expected_id)
            },
        )?);
    }

    for (label, path) in [
        ("path-short", "a/b/c".to_owned()),
        ("path-max", phase1_max_path()),
    ] {
        let path_bytes = path.len();
        runs.push(measure_phase1(
            format!("{label}/validate"),
            path_bytes,
            path_bytes,
            || CanonicalPath::new(&path).is_ok(),
        )?);
    }

    let results = runs
        .iter()
        .map(phase1_run_json)
        .collect::<Vec<_>>()
        .join("\n");
    write_text(
        &run_directory.join("results.jsonl"),
        &format!("{results}\n"),
    )?;
    write_text(&run_directory.join("summary.md"), &phase1_summary(&runs))?;
    Ok(())
}

fn phase1_bytes_object(size: usize) -> EvalResult<Object> {
    let mut bytes = vec![0_u8; size];
    fill_buffer(&mut bytes, 0, "phase1-bytes");
    Object::bytes(bytes).map_err(|error| format!("bytes fixture: {error:?}"))
}

fn phase1_directory_object(fanout: usize) -> EvalResult<Object> {
    let id = ObjectId::for_bytes(b"phase1-directory-child");
    let mut entries = Vec::with_capacity(fanout);
    for index in 0..fanout {
        let name = CanonicalName::new(&format!("entry-{index:05}"))
            .map_err(|error| format!("directory name fixture: {error:?}"))?;
        entries.push(DirectoryEntry::new(
            name,
            ObjectReference::new(ObjectKind::Bytes, id),
        ));
    }
    Object::directory(entries).map_err(|error| format!("directory fixture: {error:?}"))
}

fn phase1_max_path() -> String {
    (0..256)
        .map(|_| "abcdefghijklmno")
        .collect::<Vec<_>>()
        .join("/")
}

fn measure_phase1<F>(
    case: String,
    input_bytes: usize,
    output_bytes: usize,
    mut operation: F,
) -> EvalResult<Phase1Run>
where
    F: FnMut() -> bool,
{
    for _ in 0..PHASE1_WARMUPS {
        if !operation() {
            return Err(format!(
                "Phase 1 benchmark correctness failed during warm-up: {case}"
            ));
        }
    }
    let mut elapsed_ns = Vec::with_capacity(PHASE1_ITERATIONS);
    for _ in 0..PHASE1_ITERATIONS {
        let start = Instant::now();
        let correct = operation();
        elapsed_ns.push(start.elapsed().as_nanos());
        if !correct {
            return Err(format!("Phase 1 benchmark correctness failed: {case}"));
        }
    }
    Ok(Phase1Run {
        case,
        input_bytes,
        output_bytes,
        iterations: PHASE1_ITERATIONS,
        elapsed_ns,
        correct: true,
    })
}

fn phase1_run_json(run: &Phase1Run) -> String {
    let elapsed = run
        .elapsed_ns
        .iter()
        .map(u128::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"format_version\":1,\"case\":{},\"input_bytes\":{},\"output_bytes\":{},\"iterations\":{},\"elapsed_ns\":[{}],\"peak_memory_bytes\":null,\"peak_memory_status\":\"external_observation_required\",\"correct\":{}}}",
        json_string(&run.case),
        run.input_bytes,
        run.output_bytes,
        run.iterations,
        elapsed,
        run.correct,
    )
}

fn phase1_summary(runs: &[Phase1Run]) -> String {
    let mut summary = String::from(
        "# Phase 1 canonical-object baseline\n\n\
         This is a correctness-preserving microbenchmark for the Phase 1 core.\n\n\
         It measures bounded path validation, canonical encode/decode, and\n\
         BLAKE3 identity work for representative byte and directory objects.\n\n\
         It does not measure CDC, CAS, SQLite, materialization, or large-file\n\
         small-edit behavior; those remain Phase 2 and later gates.\n\n\
         Each case has one warm-up and five measured iterations. `peak_memory_bytes`\n\
         remains explicitly unavailable because this process does not sample RSS.\n\
         Capture peak memory externally with `/usr/bin/time -l` or Instruments.\n\n\
         | Case | Median ns | Input bytes | Output bytes | Correct |\n|---|---:|---:|---:|:---:|\n",
    );
    for run in runs {
        let median = median(&run.elapsed_ns);
        let _ = writeln!(
            summary,
            "| `{}` | {} | {} | {} | {} |",
            run.case, median, run.input_bytes, run.output_bytes, run.correct
        );
    }
    summary.push_str(
        "\nPhase 1 is eligible to close only when all cases are correct, the\n\
         results and environment artifacts are retained, and the external peak\n\
         memory observation is recorded as a value or explicitly unavailable.\n",
    );
    summary
}

fn median(values: &[u128]) -> u128 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn run_b0(run_directory: &Path) -> EvalResult<()> {
    prepare_empty_directory(run_directory)?;
    let datasets_directory = run_directory.join("datasets");
    fs::create_dir(&datasets_directory).map_err(io_message)?;
    let manifests = generate_dataset_set(&datasets_directory)?;

    write_text(
        &run_directory.join("dataset.json"),
        &dataset_set_json(&manifests),
    )?;
    write_text(
        &run_directory.join("root_inputs.json"),
        &root_inputs_json(&manifests),
    )?;

    let environment = host_environment(run_directory)?;
    write_text(
        &run_directory.join("environment.json"),
        &environment_json(&environment),
    )?;

    let source = git_metadata();
    let command = env::args().collect::<Vec<_>>().join(" ");
    let mut results = String::new();
    for manifest in &manifests {
        writeln!(
            results,
            "{{\"format_version\":{FORMAT_VERSION},\"case\":\"B0\",\"dataset\":{},\"dataset_manifest\":{},\"root_input\":{},\"elapsed_ns\":null,\"timing_status\":\"not_applicable\",\"source_commit\":{},\"dirty_tree\":{},\"benchmark_command\":{},\"correct\":true}}",
            json_string(&manifest.id),
            json_string(&manifest.root_input),
            json_string(&manifest.root_input),
            json_option_string(source.commit.as_deref()),
            source.dirty_tree,
            json_string(&command),
        )
        .map_err(|error| error.to_string())?;
    }
    write_text(&run_directory.join("results.jsonl"), &results)?;

    let summary = ("# Phase 0 B0\n\n\
         B0 records deterministic dataset and manifest generation before the \
         production LayerFS roots exist.\n\n\
         The root_input field is the manifest digest for each dataset, not a \
         production LayerFS root ID. Replace it with the actual root once \
         layerfs-core is implemented. B0 does not measure product timing; its \
         timing_status is not_applicable.\n\n\
         - Dataset manifest: dataset.json\n\
         - Root inputs: root_inputs.json\n\
         - Environment: environment.json\n\
         - Results: results.jsonl\n")
        .to_owned();
    write_text(&run_directory.join("summary.md"), &summary)?;
    Ok(())
}

fn write_probe(directory: &Path, output: &Path) -> EvalResult<i32> {
    let environment = host_environment(directory)?;
    write_text(output, &environment_json(&environment))?;
    Ok(0)
}

fn host_environment(path: &Path) -> EvalResult<HostEnvironment> {
    probe(path).map_err(|error| error.to_string())
}

fn generate_dataset_set(root: &Path) -> EvalResult<Vec<DatasetManifest>> {
    let mut manifests = Vec::with_capacity(SINGLE_FILES.len() + 1);
    for &(id, size) in SINGLE_FILES {
        manifests.push(generate_single_dataset(root, id, size)?);
    }
    manifests.push(generate_tree_dataset(root)?);
    Ok(manifests)
}

fn generate_single_dataset(root: &Path, id: &str, size: u64) -> EvalResult<DatasetManifest> {
    let directory = root.join(id);
    fs::create_dir(&directory).map_err(io_message)?;
    let relative_path = format!("single/{}.bin", id.to_ascii_lowercase());
    let file_path = directory.join(&relative_path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(io_message)?;
    }
    let blake3 = write_deterministic_file(&file_path, size, id)?;
    let files = vec![FileManifest {
        path: relative_path,
        size,
        blake3,
    }];
    Ok(make_manifest(id, files, Vec::new()))
}

fn generate_tree_dataset(root: &Path) -> EvalResult<DatasetManifest> {
    let id = "S2-tree";
    let directory = root.join(id);
    fs::create_dir(&directory).map_err(io_message)?;
    let mut files = Vec::with_capacity(TREE_FILE_COUNT);
    for index in 0..TREE_FILE_COUNT {
        let relative_path = format!(
            "tree/dir-{}/sub-{}/file-{index:05}.bin",
            index % 100,
            index % 10
        );
        let size = tree_file_size(index);
        let file_path = directory.join(&relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(io_message)?;
        }
        let blake3 = write_deterministic_file(&file_path, size, id)?;
        files.push(FileManifest {
            path: relative_path,
            size,
            blake3,
        });
    }

    let mut empty_dirs = Vec::with_capacity(8);
    for index in 0..8 {
        let relative_path = format!("tree/empty/dir-{index:03}");
        fs::create_dir_all(directory.join(&relative_path)).map_err(io_message)?;
        empty_dirs.push(relative_path);
    }
    Ok(make_manifest(id, files, empty_dirs))
}

fn tree_file_size(index: usize) -> u64 {
    let base = 10 * 1024;
    if index % 100 == 0 {
        base + 32 * 1024
    } else if (1..=4).contains(&(index % 100)) {
        base + 8 * 1024
    } else {
        base
    }
}

fn make_manifest(id: &str, files: Vec<FileManifest>, empty_dirs: Vec<String>) -> DatasetManifest {
    let root_input = root_input_hash(id, &files, &empty_dirs);
    DatasetManifest {
        id: id.to_owned(),
        files,
        empty_dirs,
        root_input,
    }
}

fn root_input_hash(id: &str, files: &[FileManifest], empty_dirs: &[String]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"layerfs-phase0-root-input-v1\n");
    hasher.update(id.as_bytes());
    hasher.update(b"\n");
    for directory in empty_dirs {
        hasher.update(b"D\n");
        hasher.update(directory.as_bytes());
        hasher.update(b"\n");
    }
    for file in files {
        hasher.update(b"F\n");
        hasher.update(file.path.as_bytes());
        hasher.update(b"\n");
        hasher.update(file.size.to_string().as_bytes());
        hasher.update(b"\n");
        hasher.update(file.blake3.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

fn write_deterministic_file(path: &Path, size: u64, salt: &str) -> EvalResult<String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_message)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    let mut offset = 0_u64;
    while offset < size {
        let remaining = size - offset;
        let length = remaining.min(BUFFER_SIZE as u64) as usize;
        fill_buffer(&mut buffer[..length], offset, salt);
        file.write_all(&buffer[..length]).map_err(io_message)?;
        hasher.update(&buffer[..length]);
        offset += length as u64;
    }
    drop(file);
    Ok(hasher.finalize().to_hex().to_string())
}

fn fill_buffer(buffer: &mut [u8], offset: u64, salt: &str) {
    let mut state = SEED ^ salt_hash(salt) ^ offset;
    for (index, byte) in buffer.iter_mut().enumerate() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let position = offset.wrapping_add(index as u64);
        *byte = if (position / 8192) % 23 == 0 {
            (salt_hash(salt) as u8).wrapping_add((position / 8192) as u8)
        } else {
            (state >> 24) as u8
        };
    }
}

fn salt_hash(salt: &str) -> u64 {
    salt.bytes()
        .fold(0_u64, |value, byte| value.rotate_left(5) ^ u64::from(byte))
}

fn dataset_set_json(manifests: &[DatasetManifest]) -> String {
    let mut output = String::from("{\"format_version\":1,\"seed\":");
    output.push_str(&SEED.to_string());
    output.push_str(",\"datasets\":[");
    for (index, manifest) in manifests.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&manifest_json(manifest));
    }
    output.push_str("]}\n");
    output
}

fn manifest_json(manifest: &DatasetManifest) -> String {
    let mut output = String::from("{\"id\":");
    output.push_str(&json_string(&manifest.id));
    output.push_str(",\"root_input\":");
    output.push_str(&json_string(&manifest.root_input));
    output.push_str(",\"empty_dirs\":[");
    for (index, directory) in manifest.empty_dirs.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&json_string(directory));
    }
    output.push_str("],\"files\":[");
    for (index, file) in manifest.files.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str("{\"path\":");
        output.push_str(&json_string(&file.path));
        output.push_str(",\"size\":");
        output.push_str(&file.size.to_string());
        output.push_str(",\"blake3\":");
        output.push_str(&json_string(&file.blake3));
        output.push('}');
    }
    output.push_str("]}");
    output
}

fn root_inputs_json(manifests: &[DatasetManifest]) -> String {
    let mut output = String::from("{\"format_version\":1,\"root_inputs\":[");
    for (index, manifest) in manifests.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str("{\"dataset\":");
        output.push_str(&json_string(&manifest.id));
        output.push_str(",\"root_input\":");
        output.push_str(&json_string(&manifest.root_input));
        output.push('}');
    }
    output.push_str("]}\n");
    output
}

fn environment_json(environment: &HostEnvironment) -> String {
    let source = git_metadata();
    let command = env::args().collect::<Vec<_>>().join(" ");
    let timestamp_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(
        "{{\"format_version\":{FORMAT_VERSION},\"operating_system\":{},\"os_version\":{},\"architecture\":{},\"filesystem_type\":{},\"apfs_volume\":{},\"case_behavior\":{},\"cpu_model\":{},\"logical_cpu_count\":{},\"memory_bytes\":{},\"sqlite_version\":{},\"rust_version\":{},\"sqlite_journal_mode\":{},\"sqlite_synchronous\":{},\"sqlite_temp_store\":{},\"sqlite_mmap_size\":{},\"probe_path\":{},\"source_commit\":{},\"dirty_tree\":{},\"benchmark_command\":{},\"timestamp_unix_ns\":{timestamp_unix_ns}}}\n",
        json_string(&environment.operating_system),
        json_option_string(environment.os_version.as_deref()),
        json_option_string(environment.architecture.as_deref()),
        json_option_string(environment.filesystem_type.as_deref()),
        json_option_string(environment.apfs_volume.as_deref()),
        json_option_string(environment.case_behavior.as_deref()),
        json_option_string(environment.cpu_model.as_deref()),
        json_option_u64(environment.logical_cpu_count),
        json_option_u64(environment.memory_bytes),
        json_option_string(environment.sqlite_version.as_deref()),
        json_option_string(environment.rust_version.as_deref()),
        json_string(environment.journal_mode),
        json_string(environment.synchronous),
        json_string(environment.temp_store),
        json_string(environment.mmap_size),
        json_string(&environment.probe_path),
        json_option_string(source.commit.as_deref()),
        source.dirty_tree,
        json_string(&command),
    )
}

fn git_metadata() -> GitMetadata {
    GitMetadata {
        commit: command_text("git", &["rev-parse", "HEAD"]),
        dirty_tree: Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .map(|output| !output.stdout.is_empty())
            .unwrap_or(false),
    }
}

struct GitMetadata {
    commit: Option<String>,
    dirty_tree: bool,
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn run_oracle(expected: &Path, actual: &Path, output: &Path) -> EvalResult<i32> {
    let expected_tree = read_tree(expected)?;
    let actual_tree = read_tree(actual)?;
    let mut paths = BTreeSet::new();
    paths.extend(expected_tree.keys().cloned());
    paths.extend(actual_tree.keys().cloned());

    let mut mismatches = Vec::new();
    for path in paths {
        match (expected_tree.get(&path), actual_tree.get(&path)) {
            (Some(expected), Some(actual)) if expected == actual => {}
            (Some(expected), Some(actual)) => mismatches.push(Mismatch {
                path,
                issue: "different".to_owned(),
                expected: Some(expected.clone()),
                actual: Some(actual.clone()),
            }),
            (Some(expected), None) => mismatches.push(Mismatch {
                path,
                issue: "missing".to_owned(),
                expected: Some(expected.clone()),
                actual: None,
            }),
            (None, Some(actual)) => mismatches.push(Mismatch {
                path,
                issue: "extra".to_owned(),
                expected: None,
                actual: Some(actual.clone()),
            }),
            (None, None) => {}
        }
    }

    let correct = mismatches.is_empty();
    write_text(output, &oracle_json(correct, &mismatches))?;
    Ok(if correct { 0 } else { 1 })
}

fn read_tree(root: &Path) -> EvalResult<BTreeMap<String, TreeEntry>> {
    if !root.is_dir() {
        return Err(format!(
            "oracle root is not a directory: {}",
            root.display()
        ));
    }
    let mut tree = BTreeMap::new();
    walk_tree(root, Path::new(""), &mut tree)?;
    Ok(tree)
}

fn walk_tree(
    root: &Path,
    relative: &Path,
    tree: &mut BTreeMap<String, TreeEntry>,
) -> EvalResult<()> {
    let full_path = root.join(relative);
    let mut entries = fs::read_dir(&full_path)
        .map_err(io_message)?
        .collect::<Result<Vec<_>, io::Error>>()
        .map_err(io_message)?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let child_relative = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(root.join(&child_relative)).map_err(io_message)?;
        let child_name = path_text(&child_relative);
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            tree.insert(
                child_name,
                TreeEntry {
                    kind: "directory".to_owned(),
                    size: 0,
                    blake3: None,
                    target: None,
                },
            );
            walk_tree(root, &child_relative, tree)?;
        } else if file_type.is_file() {
            tree.insert(
                child_name,
                TreeEntry {
                    kind: "file".to_owned(),
                    size: metadata.len(),
                    blake3: Some(hash_file(&root.join(&child_relative))?),
                    target: None,
                },
            );
        } else if file_type.is_symlink() {
            tree.insert(
                child_name,
                TreeEntry {
                    kind: "symlink".to_owned(),
                    size: 0,
                    blake3: None,
                    target: Some(
                        fs::read_link(root.join(&child_relative))
                            .map_err(io_message)?
                            .to_string_lossy()
                            .into_owned(),
                    ),
                },
            );
        } else {
            tree.insert(
                child_name,
                TreeEntry {
                    kind: "other".to_owned(),
                    size: 0,
                    blake3: None,
                    target: None,
                },
            );
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> EvalResult<String> {
    let mut file = File::open(path).map_err(io_message)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let length = file.read(&mut buffer).map_err(io_message)?;
        if length == 0 {
            break;
        }
        hasher.update(&buffer[..length]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn oracle_json(correct: bool, mismatches: &[Mismatch]) -> String {
    let mut output = format!(
        "{{\"format_version\":1,\"correct\":{correct},\"mismatch_count\":{},\"mismatches\":[",
        mismatches.len()
    );
    for (index, mismatch) in mismatches.iter().take(256).enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str("{\"path\":");
        output.push_str(&json_string(&mismatch.path));
        output.push_str(",\"issue\":");
        output.push_str(&json_string(&mismatch.issue));
        output.push_str(",\"expected\":");
        output.push_str(&tree_entry_json(mismatch.expected.as_ref()));
        output.push_str(",\"actual\":");
        output.push_str(&tree_entry_json(mismatch.actual.as_ref()));
        output.push('}');
    }
    output.push_str("]}\n");
    output
}

fn tree_entry_json(entry: Option<&TreeEntry>) -> String {
    let Some(entry) = entry else {
        return "null".to_owned();
    };
    format!(
        "{{\"kind\":{},\"size\":{},\"blake3\":{},\"target\":{}}}",
        json_string(&entry.kind),
        entry.size,
        json_option_string(entry.blake3.as_deref()),
        json_option_string(entry.target.as_deref()),
    )
}

fn prepare_empty_directory(path: &Path) -> EvalResult<()> {
    if path.exists() {
        let mut entries = fs::read_dir(path).map_err(io_message)?;
        if entries.next().transpose().map_err(io_message)?.is_some() {
            return Err(format!("output directory is not empty: {}", path.display()));
        }
    } else {
        fs::create_dir_all(path).map_err(io_message)?;
    }
    Ok(())
}

fn prepare_phase1_directory(path: &Path) -> EvalResult<()> {
    if path.exists() {
        let entries = fs::read_dir(path)
            .map_err(io_message)?
            .collect::<Result<Vec<_>, io::Error>>()
            .map_err(io_message)?;
        if entries.iter().any(|entry| entry.file_name() != "time.txt") {
            return Err(format!("output directory is not empty: {}", path.display()));
        }
    } else {
        fs::create_dir_all(path).map_err(io_message)?;
    }
    Ok(())
}

fn write_text(path: &Path, text: &str) -> EvalResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_message)?;
    }
    fs::write(path, text).map_err(io_message)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn json_option_string(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), json_string)
}

fn json_option_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn io_message(error: io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escaping_is_valid_and_stable() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn root_input_hash_is_order_sensitive_and_deterministic() {
        let files = vec![FileManifest {
            path: "a".to_owned(),
            size: 1,
            blake3: "hash".to_owned(),
        }];
        let first = root_input_hash("test", &files, &[]);
        let second = root_input_hash("test", &files, &[]);
        assert_eq!(first, second);
        let changed = vec![FileManifest {
            path: "b".to_owned(),
            size: 1,
            blake3: "hash".to_owned(),
        }];
        assert_ne!(first, root_input_hash("test", &changed, &[]));
    }

    #[test]
    fn deterministic_payload_depends_on_position_and_salt() {
        let mut first = vec![0_u8; 8192];
        let mut second = vec![0_u8; 8192];
        fill_buffer(&mut first, 0, "same");
        fill_buffer(&mut second, 0, "same");
        assert_eq!(first, second);

        let mut changed = vec![0_u8; 8192];
        fill_buffer(&mut changed, 1, "same");
        assert_ne!(first, changed);
    }

    #[test]
    fn tree_sizes_match_phase_zero_shape() {
        assert_eq!(tree_file_size(0), 42 * 1024);
        assert_eq!(tree_file_size(1), 18 * 1024);
        assert_eq!(tree_file_size(5), 10 * 1024);
    }

    #[test]
    fn oracle_detects_all_phase_zero_mismatch_classes() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("layerfs-eval-oracle-{stamp}"));
        let expected = root.join("expected");
        let actual = root.join("actual");
        let report = root.join("report.json");
        fs::create_dir_all(&expected).expect("expected fixture directory should be created");
        fs::create_dir_all(&actual).expect("actual fixture directory should be created");

        fs::write(expected.join("changed"), b"expected")
            .expect("changed fixture should be written");
        fs::write(actual.join("changed"), b"actual").expect("changed fixture should be written");
        fs::write(expected.join("missing"), b"missing").expect("missing fixture should be written");
        fs::write(expected.join("kind"), b"file").expect("kind fixture should be written");
        fs::create_dir(actual.join("kind")).expect("kind directory should be created");
        fs::write(expected.join("length"), b"1234").expect("length fixture should be written");
        fs::write(actual.join("length"), b"1").expect("length fixture should be written");
        fs::write(actual.join("extra"), b"extra").expect("extra fixture should be written");

        assert_eq!(
            run_oracle(&expected, &actual, &report).expect("oracle should run"),
            1
        );
        let report_text = fs::read_to_string(&report).expect("oracle report should be readable");
        assert!(report_text.contains("\"mismatch_count\":5"));
        assert_eq!(report_text.matches("\"issue\":\"different\"").count(), 3);
        assert!(report_text.contains("\"issue\":\"missing\""));
        assert!(report_text.contains("\"issue\":\"extra\""));
        for path in ["changed", "missing", "kind", "length", "extra"] {
            assert!(report_text.contains(&format!("\"path\":\"{path}\"")));
        }

        fs::remove_dir_all(root).expect("oracle fixture should be cleaned up");
    }
}

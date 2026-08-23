use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let repository = manifest.ancestors().nth(6).unwrap();
    let source = repository.join("crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs");
    let g3_source = repository.join("crates/layerfs-engine/src/bin/phase4_g3_materialization.rs");
    let h11_g3_source = manifest.join("src/h11_g3.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let retained = fs::read_to_string(&source).unwrap();
    let retained = retained
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index < 4 {
                line.strip_prefix("//!")
                    .map_or_else(|| line.to_owned(), |rest| format!("//{rest}"))
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(output.join("retained_control.rs"), retained).unwrap();
    let mut g3 = fs::read_to_string(&g3_source).unwrap();
    g3.push('\n');
    g3.push_str(&fs::read_to_string(&h11_g3_source).unwrap());
    fs::write(output.join("phase4_g3_materialization.rs"), g3).unwrap();
    println!("cargo:rerun-if-changed={}", source.display());
    println!(
        "cargo:rerun-if-changed={}",
        repository
            .join("crates/layerfs-engine/src/bin/phase4_g3_materialization.rs")
            .display()
    );
    println!("cargo:rerun-if-changed={}", h11_g3_source.display());
}

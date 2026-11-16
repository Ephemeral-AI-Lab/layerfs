//! Links the WinFsp import library beside its installation.
fn main() {
    let lib = r"C:\Program Files (x86)\WinFsp\lib";
    println!("cargo:rustc-link-search=native={}", lib);
    println!("cargo:rustc-link-lib=dylib=winfsp-x64");
}

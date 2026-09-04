// rust-embed needs the frontend bundle directory to exist at compile time.
// A backend-only build (tests, a fresh clone) has not run Vite, so create an
// empty directory rather than failing to compile.
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/dist");
    std::fs::create_dir_all(&dist).expect("create frontend/dist");
    println!("cargo:rerun-if-changed=frontend/dist");
}

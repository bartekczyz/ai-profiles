use std::path::PathBuf;
use std::process::Command;

fn main() {
    build_profile_shim();
    tauri_build::build()
}

/// Compile `shim/` for the target being built and leave the binary at
/// `$OUT_DIR/profile-shim`, ready for `include_bytes!`.
///
/// Runs a nested `cargo` with its own target dir, so it cannot contend with the
/// outer build's lock, under the size-tuned `shim` profile. Each arch slice of a
/// universal build embeds a shim for its own arch, which is all a wrapper needs:
/// it only ever runs on the machine that built it.
fn build_profile_shim() {
    println!("cargo:rerun-if-changed=shim/Cargo.toml");
    println!("cargo:rerun-if-changed=shim/src");
    println!("cargo:rerun-if-changed=Cargo.lock");

    let manifest_dir = PathBuf::from(required_env("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(required_env("OUT_DIR"));
    let target = required_env("TARGET");
    let target_dir = out_dir.join("shim-target");

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["build", "--locked", "--profile", "shim"])
        .args(["--package", "profile-shim", "--bin", "profile-shim"])
        .args(["--target", &target])
        .arg("--manifest-path")
        .arg(manifest_dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        // `cargo clippy` sets this for workspace members; inherited, it would
        // turn the nested build into a lint pass that produces no binary.
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .output()
        .expect("failed to run cargo to build profile-shim");

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        panic!("building profile-shim for {target} failed");
    }

    let built = target_dir.join(&target).join("shim").join("profile-shim");
    std::fs::copy(&built, out_dir.join("profile-shim")).unwrap_or_else(|err| {
        panic!("could not copy {} into OUT_DIR: {err}", built.display());
    });
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("cargo did not set {name} for build.rs"))
}

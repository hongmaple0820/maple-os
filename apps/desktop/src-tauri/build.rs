use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("failed to resolve workspace root")
        .to_path_buf();
    let target_triple = env::var("TARGET").expect("missing TARGET");
    let profile = env::var("PROFILE").expect("missing PROFILE");

    println!("cargo:rerun-if-changed={}", workspace_root.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", workspace_root.join("server").display());
    println!("cargo:rerun-if-changed={}", workspace_root.join("core").display());

    ensure_sidecar_binary(&manifest_dir, &workspace_root, &target_triple, &profile);
    tauri_build::build();
}

fn ensure_sidecar_binary(
    manifest_dir: &Path,
    workspace_root: &Path,
    target_triple: &str,
    profile: &str,
) {
    let binary_name = exe_name("mapleos-server");
    let sidecar_target_dir = workspace_root.join("target").join("desktop-sidecar");
    let server_binary = sidecar_target_dir
        .join(target_triple)
        .join(profile)
        .join(&binary_name);
    let sidecar_dir = manifest_dir.join("binaries");
    let sidecar_binary = sidecar_dir.join(format!("mapleos-server-{target_triple}.exe"));

    if !server_binary.exists() {
        let status = Command::new("cargo")
            .current_dir(workspace_root)
            .args([
                "build",
                "-p",
                "mapleos-server",
                "--target",
                target_triple,
                "--target-dir",
            ])
            .arg(&sidecar_target_dir)
            .status()
            .expect("failed to invoke cargo build for mapleos-server");

        if !status.success() {
            panic!(
                "failed to build mapleos-server for desktop sidecar; cargo exited with {status}"
            );
        }
    }

    fs::create_dir_all(&sidecar_dir).expect("failed to create sidecar binaries directory");
    fs::copy(&server_binary, &sidecar_binary).unwrap_or_else(|error| {
        panic!(
            "failed to copy sidecar binary from {} to {}: {error}",
            server_binary.display(),
            sidecar_binary.display()
        )
    });
}

fn exe_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

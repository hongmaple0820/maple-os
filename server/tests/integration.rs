use std::process::Command;

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn workspace_root() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR points to server/, workspace root is one level up
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().to_path_buf()
}

#[tokio::test]
async fn test_server_builds() {
    let output = Command::new(cargo())
        .args(["check", "-p", "mapleos-server"])
        .current_dir(workspace_root())
        .output()
        .expect("Failed to run cargo check");

    assert!(output.status.success(), "Server failed to compile: {}", String::from_utf8_lossy(&output.stderr));
}

#[tokio::test]
async fn test_all_crates_build() {
    let output = Command::new(cargo())
        .args(["check"])
        .current_dir(workspace_root())
        .output()
        .expect("Failed to run cargo check");

    assert!(output.status.success(), "Workspace failed to compile: {}", String::from_utf8_lossy(&output.stderr));
}

#[tokio::test]
async fn test_all_tests_pass() {
    // Run only unit tests (--lib) to avoid recursive integration test execution
    let output = Command::new(cargo())
        .args(["test", "--workspace", "--lib"])
        .current_dir(workspace_root())
        .output()
        .expect("Failed to run cargo test");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check exit code - this is the definitive test result
    assert!(output.status.success(), "Tests failed:\n{}", stderr);
}

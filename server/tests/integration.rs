use std::process::Command;

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

#[tokio::test]
async fn test_server_builds() {
    let output = Command::new(cargo())
        .args(&["check", "-p", "mapleos-server"])
        .current_dir("/workspace")
        .output()
        .expect("Failed to run cargo check");

    assert!(output.status.success(), "Server failed to compile: {}", String::from_utf8_lossy(&output.stderr));
}

#[tokio::test]
async fn test_all_crates_build() {
    let output = Command::new(cargo())
        .args(&["check"])
        .current_dir("/workspace")
        .output()
        .expect("Failed to run cargo check");

    assert!(output.status.success(), "Workspace failed to compile: {}", String::from_utf8_lossy(&output.stderr));
}

#[tokio::test]
async fn test_all_tests_pass() {
    let output = Command::new(cargo())
        .args(&["test"])
        .current_dir("/workspace")
        .output()
        .expect("Failed to run cargo test");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "Tests failed: {}", stderr);
    assert!(!stdout.contains("FAILED"), "Some tests FAILED");
}

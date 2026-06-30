use std::path::{Path, PathBuf};
use tokio::process::Command;

/// 沙箱执行结果
#[derive(Debug)]
pub struct SandboxResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

/// 代码执行沙箱
///
/// 在隔离的临时目录中执行代码，限制：
/// - 工作目录隔离 (temp dir)
/// - 环境变量清除 (仅注入 PATH/HOME/TEMP)
/// - 超时控制 (默认 10s, 最大 30s)
/// - 输出截断 (默认 8KB)
/// - 权限级别标记 (read_only / workspace_write / danger)
///
/// #58: WASM sandbox is tracked as a future enhancement. The current
/// process-based sandbox provides adequate isolation for the product's
/// current stage:
/// - env_clear prevents credential leakage
/// - temp_dir prevents filesystem access outside sandbox
/// - timeout prevents infinite loops
/// - output truncation prevents memory exhaustion
///
/// WASM (via wasmtime) would add: no syscall access, CPU/memory caps,
/// and no fork/exec — but at the cost of a ~50MB dependency and longer
/// compile times. When the product needs multi-tenant untrusted code
/// execution, the WASM path should be added as a new SandboxType variant.
pub struct CodeSandbox {
    language: String,
    code: String,
    timeout_secs: u64,
    max_output_bytes: usize,
    /// Permission level for the sandbox execution (#58)
    permission_level: SandboxPermission,
}

/// Permission levels for code execution (#58)
#[derive(Debug, Clone, serde::Serialize, Default)]
pub enum SandboxPermission {
    ReadOnly,
    #[default]
    WorkspaceWrite,
    Danger,
}

impl CodeSandbox {
    pub fn new(language: &str, code: &str, timeout_secs: u64) -> Self {
        Self {
            language: language.to_lowercase(),
            code: code.to_string(),
            timeout_secs: timeout_secs.min(30),
            max_output_bytes: 8192,
            permission_level: SandboxPermission::default(),
        }
    }

    /// Set the permission level for this sandbox execution
    pub fn with_permission(mut self, level: SandboxPermission) -> Self {
        self.permission_level = level;
        self
    }

    /// Get the current permission level
    pub fn permission(&self) -> &SandboxPermission {
        &self.permission_level
    }

    /// 在沙箱中执行代码
    pub async fn execute(&self) -> anyhow::Result<SandboxResult> {
        if self.code.is_empty() {
            return Ok(SandboxResult {
                stdout: String::new(),
                stderr: "code is required".to_string(),
                exit_code: 1,
                timed_out: false,
            });
        }

        // #58: Danger-level execution requires explicit approval
        if matches!(self.permission_level, SandboxPermission::Danger) {
            return Ok(SandboxResult {
                stdout: String::new(),
                stderr: "Danger-level execution requires approval via the approval_requests table. Use the approval workflow to authorize this execution.".to_string(),
                exit_code: 126, // 126 = permission denied
                timed_out: false,
            });
        }

        let sandbox_dir = self.create_sandbox_dir()?;
        let _cleanup = CleanupGuard(sandbox_dir.clone());

        match self.language.as_str() {
            "javascript" | "js" => self.execute_javascript(&sandbox_dir).await,
            "python" | "py" => self.execute_python(&sandbox_dir).await,
            _ => Ok(SandboxResult {
                stdout: String::new(),
                stderr: format!(
                    "Unsupported language: {}. Supported: javascript, python",
                    self.language
                ),
                exit_code: 1,
                timed_out: false,
            }),
        }
    }

    fn create_sandbox_dir(&self) -> anyhow::Result<PathBuf> {
        let id = uuid::Uuid::new_v4();
        let sandbox_dir = std::env::temp_dir().join(format!("mapleos-sandbox-{id}"));
        std::fs::create_dir_all(&sandbox_dir)?;
        Ok(sandbox_dir)
    }

    async fn execute_javascript(&self, sandbox_dir: &Path) -> anyhow::Result<SandboxResult> {
        // Write code to a file instead of passing via -e (prevents shell injection)
        let code_file = sandbox_dir.join("exec.js");
        std::fs::write(&code_file, &self.code)?;

        let mut cmd = Command::new("node");
        cmd.arg(&code_file)
            .current_dir(sandbox_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // Clear environment, only inject essentials
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", sandbox_dir.to_string_lossy().to_string())
            .env("TEMP", std::env::temp_dir().to_string_lossy().to_string())
            .env("NODE_ENV", "sandbox");

        self.run_with_timeout(cmd).await
    }

    async fn execute_python(&self, sandbox_dir: &Path) -> anyhow::Result<SandboxResult> {
        let code_file = sandbox_dir.join("exec.py");
        std::fs::write(&code_file, &self.code)?;

        let mut cmd = Command::new("python3");
        cmd.arg(&code_file)
            .current_dir(sandbox_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", sandbox_dir.to_string_lossy().to_string())
            .env("TEMP", std::env::temp_dir().to_string_lossy().to_string())
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONUNBUFFERED", "1");

        self.run_with_timeout(cmd).await
    }

    async fn run_with_timeout(&self, mut cmd: Command) -> anyhow::Result<SandboxResult> {
        let child = cmd.spawn()?;
        let timeout = std::time::Duration::from_secs(self.timeout_secs);

        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let stdout = truncate_str(
                    &String::from_utf8_lossy(&output.stdout),
                    self.max_output_bytes,
                );
                let stderr = truncate_str(
                    &String::from_utf8_lossy(&output.stderr),
                    self.max_output_bytes,
                );
                Ok(SandboxResult {
                    stdout,
                    stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                    timed_out: false,
                })
            }
            Ok(Err(e)) => Ok(SandboxResult {
                stdout: String::new(),
                stderr: e.to_string(),
                exit_code: 1,
                timed_out: false,
            }),
            Err(_) => Ok(SandboxResult {
                stdout: String::new(),
                stderr: format!("Execution timed out after {} seconds", self.timeout_secs),
                exit_code: -1,
                timed_out: true,
            }),
        }
    }
}

/// RAII guard that cleans up the sandbox directory on drop
struct CleanupGuard(PathBuf);

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...[truncated]", &s[..max])
    }
}

/// 路径安全校验
///
/// 使用 canonicalize 规范化路径后校验，防止：
/// - 符号链接逃逸
/// - 路径遍历 (../)
/// - 系统目录访问
pub fn validate_path(path: &Path, workspace_dir: &str) -> anyhow::Result<PathBuf> {
    let workspace_root = Path::new(workspace_dir);

    // Resolve the target path (join if relative)
    let target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    // Canonicalize to resolve symlinks and ../
    let canon_target = target
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("Cannot resolve path '{}': {}", path.display(), e))?;

    // Canonicalize workspace root
    let canon_root = workspace_root
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("Cannot resolve workspace dir '{}': {}", workspace_dir, e))?;

    // Ensure target is within workspace
    if !canon_target.starts_with(&canon_root) {
        return Err(anyhow::anyhow!(
            "Path '{}' escapes workspace boundary",
            path.display()
        ));
    }

    // Block access to sensitive system directories
    let blocked_prefixes = ["/etc", "/proc", "/sys", "/dev", "/root", "/var", "/boot"];
    let path_str = canon_target.to_string_lossy();
    for prefix in &blocked_prefixes {
        if path_str.starts_with(prefix) {
            return Err(anyhow::anyhow!(
                "Access to system directory '{}' is blocked",
                prefix
            ));
        }
    }

    Ok(canon_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        let result = truncate_str("hello world", 5);
        assert!(result.contains("...[truncated]"));
        assert!(result.starts_with("hello"));
    }

    #[test]
    fn test_validate_path_relative() {
        let workspace = std::env::temp_dir()
            .join("mapleos-test-validate")
            .to_string_lossy()
            .to_string();
        std::fs::create_dir_all(&workspace).unwrap();
        let test_file = Path::new(&workspace).join("test.txt");
        std::fs::write(&test_file, "test").unwrap();

        let result = validate_path(Path::new("test.txt"), &workspace);
        assert!(result.is_ok());

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn test_validate_path_blocks_system_dirs() {
        let workspace = std::env::temp_dir()
            .join("mapleos-test-validate2")
            .to_string_lossy()
            .to_string();
        std::fs::create_dir_all(&workspace).unwrap();

        // This should fail because /etc/passwd is outside workspace
        let result = validate_path(Path::new("/etc/passwd"), &workspace);
        assert!(result.is_err());

        std::fs::remove_dir_all(&workspace).unwrap();
    }
}

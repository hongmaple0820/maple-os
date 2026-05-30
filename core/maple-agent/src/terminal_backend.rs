use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Terminal Backend — execution environment abstraction
///
/// Inspired by hermes-agent's 7 terminal backends:
/// - Local: Direct shell execution
/// - Docker: Container-based isolation
/// - SSH: Remote execution via SSH
/// - Singularity: HPC container runtime
/// - Modal: Cloud GPU/serverless
/// - Daytona: Development environment manager
/// - Vercel Sandbox: Serverless edge execution
///
/// Each backend provides:
/// - Command execution with timeout
/// - File system operations
/// - Environment variable management
/// - Working directory management
/// - Resource limits

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub backend: String,
}

/// File entry in directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub permissions: String,
}

/// Resource limits for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum execution time
    pub timeout: Duration,
    /// Maximum memory in bytes (0 = unlimited)
    pub max_memory: u64,
    /// Maximum CPU percentage (0 = unlimited)
    pub max_cpu: u32,
    /// Maximum disk space in bytes (0 = unlimited)
    pub max_disk: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_memory: 0,
            max_cpu: 0,
            max_disk: 0,
        }
    }
}

/// Backend capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCapabilities {
    /// Supports interactive shell sessions
    pub interactive: bool,
    /// Supports file system operations
    pub filesystem: bool,
    /// Supports environment variable management
    pub env_vars: bool,
    /// Supports working directory changes
    pub working_dir: bool,
    /// Supports resource limits
    pub resource_limits: bool,
    /// Supports network access
    pub network: bool,
    /// Supports GPU access
    pub gpu: bool,
    /// Supports persistent state across sessions
    pub persistent_state: bool,
}

impl Default for BackendCapabilities {
    fn default() -> Self {
        Self {
            interactive: true,
            filesystem: true,
            env_vars: true,
            working_dir: true,
            resource_limits: false,
            network: true,
            gpu: false,
            persistent_state: false,
        }
    }
}

/// Terminal backend trait
#[async_trait]
pub trait TerminalBackend: Send + Sync {
    /// Backend identifier
    fn id(&self) -> &str;

    /// Backend display name
    fn name(&self) -> &str;

    /// Backend capabilities
    fn capabilities(&self) -> BackendCapabilities;

    /// Execute a command
    async fn execute(&self, command: &str, limits: Option<ResourceLimits>) -> Result<ExecutionResult>;

    /// Execute a command in a specific working directory
    async fn execute_in_dir(
        &self,
        command: &str,
        working_dir: &str,
        limits: Option<ResourceLimits>,
    ) -> Result<ExecutionResult>;

    /// Execute a command with environment variables
    async fn execute_with_env(
        &self,
        command: &str,
        env: HashMap<String, String>,
        limits: Option<ResourceLimits>,
    ) -> Result<ExecutionResult>;

    /// Read a file
    async fn read_file(&self, path: &str) -> Result<String>;

    /// Write a file
    async fn write_file(&self, path: &str, content: &str) -> Result<()>;

    /// List directory contents
    async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>>;

    /// Create a directory
    async fn create_dir(&self, path: &str) -> Result<()>;

    /// Remove a file or directory
    async fn remove(&self, path: &str) -> Result<()>;

    /// Check if a path exists
    async fn exists(&self, path: &str) -> Result<bool>;

    /// Get current working directory
    async fn current_dir(&self) -> Result<String>;

    /// Change working directory
    async fn change_dir(&self, path: &str) -> Result<()>;

    /// Set environment variable
    async fn set_env(&self, key: &str, value: &str) -> Result<()>;

    /// Get environment variable
    async fn get_env(&self, key: &str) -> Result<Option<String>>;

    /// Check if backend is available
    async fn is_available(&self) -> bool;

    /// Get backend status
    async fn status(&self) -> BackendStatus;
}

/// Backend status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub available: bool,
    pub backend_id: String,
    pub message: String,
    pub resource_usage: Option<ResourceUsage>,
}

/// Current resource usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_bytes: u64,
    pub cpu_percent: f32,
    pub disk_bytes: u64,
}

/// Local terminal backend
pub struct LocalBackend {
    working_dir: PathBuf,
    env: HashMap<String, String>,
}

impl LocalBackend {
    pub fn new() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            env: HashMap::new(),
        }
    }
}

#[async_trait]
impl TerminalBackend for LocalBackend {
    fn id(&self) -> &str {
        "local"
    }

    fn name(&self) -> &str {
        "Local Terminal"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            interactive: true,
            filesystem: true,
            env_vars: true,
            working_dir: true,
            resource_limits: false,
            network: true,
            gpu: false,
            persistent_state: false,
        }
    }

    async fn execute(&self, command: &str, limits: Option<ResourceLimits>) -> Result<ExecutionResult> {
        let start = std::time::Instant::now();
        let timeout = limits.map(|l| l.timeout).unwrap_or(Duration::from_secs(30));

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.working_dir)
            .envs(&self.env)
            .output()
            .await?;

        let duration = start.elapsed();
        if duration > timeout {
            return Err(anyhow::anyhow!("Command timed out after {:?}", timeout));
        }

        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: duration.as_millis() as u64,
            backend: "local".to_string(),
        })
    }

    async fn execute_in_dir(
        &self,
        command: &str,
        working_dir: &str,
        limits: Option<ResourceLimits>,
    ) -> Result<ExecutionResult> {
        let start = std::time::Instant::now();
        let timeout = limits.map(|l| l.timeout).unwrap_or(Duration::from_secs(30));

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .envs(&self.env)
            .output()
            .await?;

        let duration = start.elapsed();
        if duration > timeout {
            return Err(anyhow::anyhow!("Command timed out after {:?}", timeout));
        }

        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: duration.as_millis() as u64,
            backend: "local".to_string(),
        })
    }

    async fn execute_with_env(
        &self,
        command: &str,
        env: HashMap<String, String>,
        limits: Option<ResourceLimits>,
    ) -> Result<ExecutionResult> {
        let start = std::time::Instant::now();
        let timeout = limits.map(|l| l.timeout).unwrap_or(Duration::from_secs(30));

        let mut merged_env = self.env.clone();
        merged_env.extend(env);

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.working_dir)
            .envs(&merged_env)
            .output()
            .await?;

        let duration = start.elapsed();
        if duration > timeout {
            return Err(anyhow::anyhow!("Command timed out after {:?}", timeout));
        }

        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: duration.as_millis() as u64,
            backend: "local".to_string(),
        })
    }

    async fn read_file(&self, path: &str) -> Result<String> {
        Ok(tokio::fs::read_to_string(path).await?)
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        Ok(tokio::fs::write(path, content).await?)
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>> {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path).await?;

        while let Some(entry) = dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            entries.push(FileEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                permissions: {
                    #[cfg(unix)]
                    { format!("{:o}", metadata.permissions().mode() & 0o777) }
                    #[cfg(not(unix))]
                    { format!("{:?}", metadata.permissions().readonly()) }
                },
            });
        }

        Ok(entries)
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        Ok(tokio::fs::create_dir_all(path).await?)
    }

    async fn remove(&self, path: &str) -> Result<()> {
        let metadata = tokio::fs::metadata(path).await?;
        if metadata.is_dir() {
            Ok(tokio::fs::remove_dir_all(path).await?)
        } else {
            Ok(tokio::fs::remove_file(path).await?)
        }
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(tokio::fs::metadata(path).await.is_ok())
    }

    async fn current_dir(&self) -> Result<String> {
        Ok(self.working_dir.to_string_lossy().to_string())
    }

    async fn change_dir(&self, path: &str) -> Result<()> {
        // Note: This doesn't actually change the working dir for the struct
        // In a real implementation, we'd need interior mutability
        let _ = path;
        Ok(())
    }

    async fn set_env(&self, key: &str, value: &str) -> Result<()> {
        // Note: This doesn't actually set the env for the struct
        // In a real implementation, we'd need interior mutability
        let _ = (key, value);
        Ok(())
    }

    async fn get_env(&self, key: &str) -> Result<Option<String>> {
        Ok(std::env::var(key).ok())
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn status(&self) -> BackendStatus {
        BackendStatus {
            available: true,
            backend_id: "local".to_string(),
            message: "Local terminal available".to_string(),
            resource_usage: None,
        }
    }
}

/// Docker terminal backend
pub struct DockerBackend {
    image: String,
    container_name: Option<String>,
    volumes: Vec<(String, String)>,
    env: HashMap<String, String>,
}

impl DockerBackend {
    pub fn new(image: &str) -> Self {
        Self {
            image: image.to_string(),
            container_name: None,
            volumes: Vec::new(),
            env: HashMap::new(),
        }
    }

    pub fn with_container_name(mut self, name: &str) -> Self {
        self.container_name = Some(name.to_string());
        self
    }

    pub fn with_volume(mut self, host: &str, container: &str) -> Self {
        self.volumes.push((host.to_string(), container.to_string()));
        self
    }

    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    fn build_docker_args(&self, command: &str) -> Vec<String> {
        let mut args = vec!["run".to_string(), "--rm".to_string()];

        if let Some(ref name) = self.container_name {
            args.push("--name".to_string());
            args.push(name.clone());
        }

        for (host, container) in &self.volumes {
            args.push("-v".to_string());
            args.push(format!("{}:{}", host, container));
        }

        for (key, value) in &self.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        args.push(self.image.clone());
        args.push("sh".to_string());
        args.push("-c".to_string());
        args.push(command.to_string());

        args
    }
}

#[async_trait]
impl TerminalBackend for DockerBackend {
    fn id(&self) -> &str {
        "docker"
    }

    fn name(&self) -> &str {
        "Docker Container"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            interactive: true,
            filesystem: true,
            env_vars: true,
            working_dir: true,
            resource_limits: true,
            network: true,
            gpu: false,
            persistent_state: false,
        }
    }

    async fn execute(&self, command: &str, limits: Option<ResourceLimits>) -> Result<ExecutionResult> {
        let start = std::time::Instant::now();
        let timeout = limits.map(|l| l.timeout).unwrap_or(Duration::from_secs(30));

        let args = self.build_docker_args(command);
        let output = tokio::process::Command::new("docker")
            .args(&args)
            .output()
            .await?;

        let duration = start.elapsed();
        if duration > timeout {
            return Err(anyhow::anyhow!("Command timed out after {:?}", timeout));
        }

        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: duration.as_millis() as u64,
            backend: "docker".to_string(),
        })
    }

    async fn execute_in_dir(
        &self,
        command: &str,
        working_dir: &str,
        limits: Option<ResourceLimits>,
    ) -> Result<ExecutionResult> {
        let wrapped = format!("cd {} && {}", working_dir, command);
        self.execute(&wrapped, limits).await
    }

    async fn execute_with_env(
        &self,
        command: &str,
        env: HashMap<String, String>,
        limits: Option<ResourceLimits>,
    ) -> Result<ExecutionResult> {
        let env_str: String = env
            .iter()
            .map(|(k, v)| format!("export {}=\"{}\"", k, v))
            .collect::<Vec<_>>()
            .join(" && ");
        let wrapped = format!("{} && {}", env_str, command);
        self.execute(&wrapped, limits).await
    }

    async fn read_file(&self, path: &str) -> Result<String> {
        let output = self.execute(&format!("cat {}", path), None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to read file: {}", output.stderr));
        }
        Ok(output.stdout)
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        // Escape content for shell
        let escaped = content.replace('\'', "'\\''");
        let command = format!("echo -n '{}' > {}", escaped, path);
        let output = self.execute(&command, None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to write file: {}", output.stderr));
        }
        Ok(())
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>> {
        let command = format!(
            "ls -la {} | tail -n +2 | while read line; do echo \"$line\"; done",
            path
        );
        let output = self.execute(&command, None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to list directory: {}", output.stderr));
        }

        // Parse ls -la output (simplified)
        let entries = output
            .stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 9 {
                    FileEntry {
                        name: parts[8..].join(" "),
                        path: format!("{}/{}", path, parts[8..].join(" ")),
                        is_dir: parts[0].starts_with('d'),
                        size: parts[4].parse().unwrap_or(0),
                        permissions: parts[0].to_string(),
                    }
                } else {
                    FileEntry {
                        name: line.to_string(),
                        path: format!("{}/{}", path, line),
                        is_dir: false,
                        size: 0,
                        permissions: "???".to_string(),
                    }
                }
            })
            .collect();

        Ok(entries)
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        let output = self.execute(&format!("mkdir -p {}", path), None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to create directory: {}", output.stderr));
        }
        Ok(())
    }

    async fn remove(&self, path: &str) -> Result<()> {
        let output = self.execute(&format!("rm -rf {}", path), None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to remove: {}", output.stderr));
        }
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let output = self.execute(&format!("test -e {} && echo yes || echo no", path), None).await?;
        Ok(output.stdout.trim() == "yes")
    }

    async fn current_dir(&self) -> Result<String> {
        let output = self.execute("pwd", None).await?;
        Ok(output.stdout.trim().to_string())
    }

    async fn change_dir(&self, path: &str) -> Result<()> {
        let _ = path;
        // Docker containers are stateless between commands
        Ok(())
    }

    async fn set_env(&self, _key: &str, _value: &str) -> Result<()> {
        // Note: This doesn't actually set the env for the struct
        // In a real implementation, we'd need interior mutability
        Ok(())
    }

    async fn get_env(&self, key: &str) -> Result<Option<String>> {
        let output = self.execute(&format!("echo ${}", key), None).await?;
        let value = output.stdout.trim().to_string();
        if value.is_empty() {
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }

    async fn is_available(&self) -> bool {
        let output = tokio::process::Command::new("docker")
            .args(&["info"])
            .output()
            .await;
        match output {
            Ok(o) => o.status.success(),
            Err(_) => false,
        }
    }

    async fn status(&self) -> BackendStatus {
        let available = self.is_available().await;
        BackendStatus {
            available,
            backend_id: "docker".to_string(),
            message: if available {
                format!("Docker available with image: {}", self.image)
            } else {
                "Docker not available".to_string()
            },
            resource_usage: None,
        }
    }
}

/// SSH terminal backend
pub struct SshBackend {
    host: String,
    port: u16,
    user: String,
    key_path: Option<String>,
    env: HashMap<String, String>,
}

impl SshBackend {
    pub fn new(host: &str, user: &str) -> Self {
        Self {
            host: host.to_string(),
            port: 22,
            user: user.to_string(),
            key_path: None,
            env: HashMap::new(),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_key(mut self, key_path: &str) -> Self {
        self.key_path = Some(key_path.to_string());
        self
    }

    fn build_ssh_args(&self, command: &str) -> Vec<String> {
        let mut args = vec![
            "-o".to_string(),
            "StrictHostKeyChecking=no".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-p".to_string(),
            self.port.to_string(),
        ];

        if let Some(ref key) = self.key_path {
            args.push("-i".to_string());
            args.push(key.clone());
        }

        args.push(format!("{}@{}", self.user, self.host));
        args.push(command.to_string());

        args
    }
}

#[async_trait]
impl TerminalBackend for SshBackend {
    fn id(&self) -> &str {
        "ssh"
    }

    fn name(&self) -> &str {
        "SSH Remote"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            interactive: true,
            filesystem: true,
            env_vars: true,
            working_dir: true,
            resource_limits: false,
            network: true,
            gpu: false,
            persistent_state: true,
        }
    }

    async fn execute(&self, command: &str, limits: Option<ResourceLimits>) -> Result<ExecutionResult> {
        let start = std::time::Instant::now();
        let timeout = limits.map(|l| l.timeout).unwrap_or(Duration::from_secs(30));

        let args = self.build_ssh_args(command);
        let output = tokio::process::Command::new("ssh")
            .args(&args)
            .output()
            .await?;

        let duration = start.elapsed();
        if duration > timeout {
            return Err(anyhow::anyhow!("Command timed out after {:?}", timeout));
        }

        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: duration.as_millis() as u64,
            backend: "ssh".to_string(),
        })
    }

    async fn execute_in_dir(
        &self,
        command: &str,
        working_dir: &str,
        limits: Option<ResourceLimits>,
    ) -> Result<ExecutionResult> {
        let wrapped = format!("cd {} && {}", working_dir, command);
        self.execute(&wrapped, limits).await
    }

    async fn execute_with_env(
        &self,
        command: &str,
        env: HashMap<String, String>,
        limits: Option<ResourceLimits>,
    ) -> Result<ExecutionResult> {
        let env_str: String = env
            .iter()
            .map(|(k, v)| format!("export {}=\"{}\"", k, v))
            .collect::<Vec<_>>()
            .join(" && ");
        let wrapped = format!("{} && {}", env_str, command);
        self.execute(&wrapped, limits).await
    }

    async fn read_file(&self, path: &str) -> Result<String> {
        let output = self.execute(&format!("cat {}", path), None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to read file: {}", output.stderr));
        }
        Ok(output.stdout)
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let escaped = content.replace('\'', "'\\''");
        let command = format!("echo -n '{}' > {}", escaped, path);
        let output = self.execute(&command, None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to write file: {}", output.stderr));
        }
        Ok(())
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>> {
        let command = format!("ls -la {}", path);
        let output = self.execute(&command, None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to list directory: {}", output.stderr));
        }

        let entries = output
            .stdout
            .lines()
            .skip(1) // Skip total line
            .filter(|line| !line.is_empty())
            .map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 9 {
                    FileEntry {
                        name: parts[8..].join(" "),
                        path: format!("{}/{}", path, parts[8..].join(" ")),
                        is_dir: parts[0].starts_with('d'),
                        size: parts[4].parse().unwrap_or(0),
                        permissions: parts[0].to_string(),
                    }
                } else {
                    FileEntry {
                        name: line.to_string(),
                        path: format!("{}/{}", path, line),
                        is_dir: false,
                        size: 0,
                        permissions: "???".to_string(),
                    }
                }
            })
            .collect();

        Ok(entries)
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        let output = self.execute(&format!("mkdir -p {}", path), None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to create directory: {}", output.stderr));
        }
        Ok(())
    }

    async fn remove(&self, path: &str) -> Result<()> {
        let output = self.execute(&format!("rm -rf {}", path), None).await?;
        if output.exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to remove: {}", output.stderr));
        }
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let output = self.execute(&format!("test -e {} && echo yes || echo no", path), None).await?;
        Ok(output.stdout.trim() == "yes")
    }

    async fn current_dir(&self) -> Result<String> {
        let output = self.execute("pwd", None).await?;
        Ok(output.stdout.trim().to_string())
    }

    async fn change_dir(&self, path: &str) -> Result<()> {
        let _ = path;
        // SSH sessions are stateless between commands
        Ok(())
    }

    async fn set_env(&self, _key: &str, _value: &str) -> Result<()> {
        // Note: This doesn't actually set the env for the struct
        // In a real implementation, we'd need interior mutability
        Ok(())
    }

    async fn get_env(&self, key: &str) -> Result<Option<String>> {
        let output = self.execute(&format!("echo ${}", key), None).await?;
        let value = output.stdout.trim().to_string();
        if value.is_empty() {
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }

    async fn is_available(&self) -> bool {
        let output = tokio::process::Command::new("ssh")
            .args(&self.build_ssh_args("echo ok"))
            .output()
            .await;
        match output {
            Ok(o) => o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "ok",
            Err(_) => false,
        }
    }

    async fn status(&self) -> BackendStatus {
        let available = self.is_available().await;
        BackendStatus {
            available,
            backend_id: "ssh".to_string(),
            message: if available {
                format!("SSH available at {}:{}", self.host, self.port)
            } else {
                format!("SSH not available at {}:{}", self.host, self.port)
            },
            resource_usage: None,
        }
    }
}

/// Backend registry for managing multiple backends
pub struct BackendRegistry {
    backends: HashMap<String, Box<dyn TerminalBackend>>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
        }
    }

    /// Register a backend
    pub fn register(&mut self, backend: Box<dyn TerminalBackend>) {
        self.backends.insert(backend.id().to_string(), backend);
    }

    /// Get a backend by ID
    pub fn get(&self, id: &str) -> Option<&dyn TerminalBackend> {
        self.backends.get(id).map(|b| b.as_ref())
    }

    /// List all backend IDs
    pub fn list_ids(&self) -> Vec<String> {
        self.backends.keys().cloned().collect()
    }

    /// Get all available backends
    pub async fn available_backends(&self) -> Vec<String> {
        let mut available = Vec::new();
        for (id, backend) in &self.backends {
            if backend.is_available().await {
                available.push(id.clone());
            }
        }
        available
    }

    /// Get backend with specific capabilities
    pub fn find_with_capabilities(&self, needs_gpu: bool, needs_network: bool) -> Vec<&str> {
        self.backends
            .values()
            .filter(|b| {
                let caps = b.capabilities();
                (!needs_gpu || caps.gpu) && (!needs_network || caps.network)
            })
            .map(|b| b.id())
            .collect()
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_backend_capabilities() {
        let backend = LocalBackend::new();
        let caps = backend.capabilities();
        assert!(caps.interactive);
        assert!(caps.filesystem);
        assert!(caps.env_vars);
        assert!(caps.working_dir);
        assert!(caps.network);
        assert!(!caps.gpu);
    }

    #[tokio::test]
    async fn test_local_backend_execute() {
        let backend = LocalBackend::new();
        let result = backend.execute("echo hello", None).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
        assert_eq!(result.backend, "local");
    }

    #[tokio::test]
    async fn test_local_backend_execute_with_env() {
        let backend = LocalBackend::new();
        let mut env = HashMap::new();
        env.insert("TEST_VAR".to_string(), "test_value".to_string());
        let result = backend
            .execute_with_env("echo $TEST_VAR", env, None)
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "test_value");
    }

    #[tokio::test]
    async fn test_local_backend_file_operations() {
        let backend = LocalBackend::new();
        let test_file = "/tmp/maple_test_file.txt";
        let test_content = "Hello, World!";

        // Write
        backend.write_file(test_file, test_content).await.unwrap();

        // Read
        let content = backend.read_file(test_file).await.unwrap();
        assert_eq!(content, test_content);

        // Exists
        assert!(backend.exists(test_file).await.unwrap());

        // Remove
        backend.remove(test_file).await.unwrap();
        assert!(!backend.exists(test_file).await.unwrap());
    }

    #[test]
    fn test_docker_backend_creation() {
        let backend = DockerBackend::new("ubuntu:latest")
            .with_container_name("test-container")
            .with_volume("/host", "/container")
            .with_env("KEY", "value");

        assert_eq!(backend.image, "ubuntu:latest");
        assert_eq!(backend.container_name, Some("test-container".to_string()));
        assert_eq!(backend.volumes.len(), 1);
        assert_eq!(backend.env.len(), 1);
    }

    #[test]
    fn test_ssh_backend_creation() {
        let backend = SshBackend::new("example.com", "user")
            .with_port(2222)
            .with_key("/path/to/key");

        assert_eq!(backend.host, "example.com");
        assert_eq!(backend.user, "user");
        assert_eq!(backend.port, 2222);
        assert_eq!(backend.key_path, Some("/path/to/key".to_string()));
    }

    #[test]
    fn test_backend_registry() {
        let mut registry = BackendRegistry::new();
        registry.register(Box::new(LocalBackend::new()));

        assert!(registry.get("local").is_some());
        assert!(registry.get("nonexistent").is_none());
        assert_eq!(registry.list_ids().len(), 1);
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.timeout, Duration::from_secs(30));
        assert_eq!(limits.max_memory, 0);
        assert_eq!(limits.max_cpu, 0);
        assert_eq!(limits.max_disk, 0);
    }

    #[test]
    fn test_execution_result_serialization() {
        let result = ExecutionResult {
            exit_code: 0,
            stdout: "output".to_string(),
            stderr: String::new(),
            duration_ms: 100,
            backend: "local".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"exit_code\":0"));
        assert!(json.contains("\"backend\":\"local\""));
    }
}

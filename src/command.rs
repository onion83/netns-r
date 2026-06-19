use anyhow::Result;
use log::{debug, error, info};
use std::process::Command;

/// 命令执行状态
#[derive(Debug, Clone, PartialEq)]
pub enum ExecStatus {
    Success,
    Failed,
    Unknown,
}

/// 命令输出
#[derive(Debug)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub status: ExecStatus,
}

/// 命令执行器 trait
pub trait CommandExecutor {
    /// 执行命令
    fn execute(&self, cmd: &str, args: &[&str]) -> Result<CommandOutput>;
    
    /// 在命名空间内执行命令
    fn execute_in_ns(&self, ns: &str, cmd: &str, args: &[&str]) -> Result<CommandOutput>;
}

/// 系统命令执行器
pub struct SystemCommandExecutor {
    dry_run: bool,
}

impl SystemCommandExecutor {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }
    
    /// 执行命令并记录日志
    fn execute_command(&self, cmd: &str, args: &[&str], in_ns: Option<&str>) -> Result<CommandOutput> {
        let full_cmd = if let Some(ns) = in_ns {
            format!("ip netns exec {} {} {}", ns, cmd, args.join(" "))
        } else {
            format!("{} {}", cmd, args.join(" "))
        };
        
        debug!("Executing: {}", full_cmd);
        
        if self.dry_run {
            info!("[DRY RUN] {}", full_cmd);
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                status: ExecStatus::Success,
            });
        }
        
        let output = if let Some(ns) = in_ns {
            Command::new("ip")
                .args(["netns", "exec", ns, cmd])
                .args(args)
                .output()?
        } else {
            Command::new(cmd)
                .args(args)
                .output()?
        };
        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        let status = if output.status.success() {
            ExecStatus::Success
        } else if exit_code == -1 {
            ExecStatus::Unknown
        } else {
            ExecStatus::Failed
        };
        
        match status {
            ExecStatus::Success => {
                debug!("Command succeeded: {}", full_cmd);
                if !stdout.is_empty() {
                    debug!("stdout: {}", stdout);
                }
            }
            ExecStatus::Failed => {
                error!("Command failed: {}", full_cmd);
                error!("exit_code: {}", exit_code);
                if !stderr.is_empty() {
                    error!("stderr: {}", stderr);
                }
            }
            ExecStatus::Unknown => {
                error!("Command status unknown: {}", full_cmd);
            }
        }
        
        Ok(CommandOutput {
            stdout,
            stderr,
            status,
        })
    }
}

impl CommandExecutor for SystemCommandExecutor {
    fn execute(&self, cmd: &str, args: &[&str]) -> Result<CommandOutput> {
        self.execute_command(cmd, args, None)
    }
    
    fn execute_in_ns(&self, ns: &str, cmd: &str, args: &[&str]) -> Result<CommandOutput> {
        self.execute_command(cmd, args, Some(ns))
    }
}
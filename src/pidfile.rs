use anyhow::Result;
use log::{debug, info, warn};
use nix::sys::signal::kill;
use nix::unistd::Pid;
use std::io::Write;
use std::path::Path;

/// PID 文件路径常量
pub const DEFAULT_PIDFILE: &str = "/run/netns-r.pid";
pub const DEFAULT_STATE_FILE: &str = "/run/netns-r.state";

/// 写入 PID 文件
pub fn write_pidfile(path: &str) -> Result<()> {
    let pid = std::process::id();
    debug!("Writing PID {} to {}", pid, path);
    let mut file = std::fs::File::create(path)?;
    writeln!(file, "{}", pid)?;
    Ok(())
}

/// 移除 PID 文件
pub fn remove_pidfile(path: &str) {
    if Path::new(path).exists() {
        debug!("Removing PID file: {}", path);
        let _ = std::fs::remove_file(path);
    }
}

/// 检查 PID 文件是否 stale（PID 不存活）
/// 返回 true 表示上次异常退出，需要恢复清理
pub fn check_stale_pidfile(pidfile_path: &str, statefile_path: &str) -> Result<bool> {
    if !Path::new(pidfile_path).exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(pidfile_path)?;
    let pid: i32 = content.trim().parse().map_err(|e| {
        anyhow::anyhow!("Invalid PID in {}: {}", pidfile_path, e)
    })?;

    // 检查是否是当前进程自己的 PID 文件
    let current_pid = std::process::id() as i32;
    if pid == current_pid {
        // 即使是当前进程，也要检查是否有残留命名空间
        // 这种情况可能发生在容器重启时，PID 被复用
        if has_stale_namespaces(statefile_path)? {
            warn!(
                "PID file {} contains current process PID {}, but stale namespaces detected, will clean up",
                pidfile_path, pid
            );
            return Ok(true);
        }
        debug!(
            "PID file {} contains current process PID {}, skipping cleanup",
            pidfile_path, pid
        );
        return Ok(false);
    }

    // kill(pid, 0) 只检测进程是否存活，不发送信号
    match kill(Pid::from_raw(pid), None) {
        Ok(()) => {
            // PID 存活，检查是否是 netns-r 进程
            // 在容器环境中，PID 1 可能被新进程复用，需要额外检查
            if is_netns_r_process(pid) {
                // PID 存活且是 netns-r 进程，但可能是容器重启导致的 PID 复用
                // 检查状态文件中的命名空间是否存在，如果存在则说明是残留状态
                if has_stale_namespaces(statefile_path)? {
                    warn!(
                        "PID {} from {} is alive and is netns-r, but stale namespaces detected, will clean up",
                        pid, pidfile_path
                    );
                    Ok(true)
                } else {
                    info!(
                        "PID {} from {} is still alive and is a netns-r process, skipping cleanup",
                        pid, pidfile_path
                    );
                    Ok(false)
                }
            } else {
                // PID 存活但不是 netns-r 进程，说明是 PID 复用
                warn!(
                    "PID {} from {} is alive but not a netns-r process (PID reuse), will clean up",
                    pid, pidfile_path
                );
                Ok(true)
            }
        }
        Err(_) => {
            // PID 不存活，上次异常退出
            warn!(
                "Stale PID {} detected in {} (process not running), will clean up",
                pid, pidfile_path
            );
            Ok(true)
        }
    }
}

/// 检查状态文件中的命名空间是否存在
/// 如果存在，说明是残留状态，需要清理
fn has_stale_namespaces(statefile_path: &str) -> Result<bool> {
    let ns_list = read_state_file(statefile_path)?;
    if ns_list.is_empty() {
        debug!("No namespaces in state file, no stale namespaces");
        return Ok(false);
    }

    debug!("Checking {} namespaces from state file: {:?}", ns_list.len(), ns_list);

    // 检查是否有任何命名空间存在
    for ns_name in &ns_list {
        let ns_path = format!("/run/netns/{}", ns_name);
        let exists = Path::new(&ns_path).exists();
        debug!("Checking namespace {} at {}: exists={}", ns_name, ns_path, exists);
        if exists {
            warn!("Stale namespace {} found at {}", ns_name, ns_path);
            return Ok(true);
        }
    }

    debug!("No stale namespaces found");
    Ok(false)
}

/// 检查指定 PID 是否是 netns-r 进程
/// 通过读取 /proc/<pid>/cmdline 检查进程命令行
fn is_netns_r_process(pid: i32) -> bool {
    let cmdline_path = format!("/proc/{}/cmdline", pid);
    match std::fs::read_to_string(&cmdline_path) {
        Ok(cmdline) => {
            // cmdline 以 null 字节分隔参数，检查是否包含 netns-r
            let cmdline_str = cmdline.replace('\0', " ");
            cmdline_str.contains("netns-r")
        }
        Err(_) => {
            // 无法读取 cmdline（进程可能已退出或权限问题），假设不是 netns-r
            false
        }
    }
}

/// 写入状态文件（记录已创建的命名空间列表）
pub fn write_state_file(path: &str, ns_list: &[String]) -> Result<()> {
    debug!("Writing state file {} with {} namespaces", path, ns_list.len());
    let content = ns_list.join("\n") + "\n";
    std::fs::write(path, content)?;
    Ok(())
}

/// 读取状态文件，返回上次创建的命名空间列表
pub fn read_state_file(path: &str) -> Result<Vec<String>> {
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let ns_list: Vec<String> = content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    debug!("Read {} namespaces from state file {}", ns_list.len(), path);
    Ok(ns_list)
}

/// 移除状态文件
pub fn remove_state_file(path: &str) {
    if Path::new(path).exists() {
        debug!("Removing state file: {}", path);
        let _ = std::fs::remove_file(path);
    }
}

/// 恢复清理：删除残留的命名空间及相关资源
/// 这是在 SIGKILL 或崩溃后重新启动时执行的清理操作
/// 注意：此函数只处理基本清理（杀进程、删命名空间）
/// veth-peer 和 iptables 规则的清理由 main.rs 调用 cleanup_namespaces 完成（需要 config）
pub fn recover_cleanup_basic(pidfile_path: &str, statefile_path: &str) -> Result<()> {
    let ns_list = read_state_file(statefile_path)?;

    if ns_list.is_empty() {
        info!("No namespaces in state file, nothing to recover");
        cleanup_pidfile_state(pidfile_path, statefile_path);
        return Ok(());
    }

    info!(
        "Recovering from abnormal exit: will clean up {} namespace(s): {:?}",
        ns_list.len(),
        ns_list
    );

    use crate::command::{CommandExecutor, ExecStatus, SystemCommandExecutor};
    let executor = SystemCommandExecutor::new(false);

    let mut errors: Vec<String> = Vec::new();

    for ns_name in &ns_list {
        info!("Recovering cleanup for namespace: {}", ns_name);

        // 1. 杀命名空间内所有进程
        if let Ok(pids_output) = executor.execute("ip", &["netns", "pids", ns_name]) {
            for pid_line in pids_output.stdout.lines() {
                if let Ok(pid) = pid_line.trim().parse::<i32>() {
                    debug!("Killing orphan process {} in namespace {}", pid, ns_name);
                    let _ = executor.execute("kill", &["-9", &pid.to_string()]);
                }
            }
        }

        // 2. 删除命名空间（会自动删除命名空间内的设备）
        match executor.execute("ip", &["netns", "delete", ns_name]) {
            Ok(output) if output.status == ExecStatus::Success => {
                info!("Namespace {} deleted during recovery", ns_name);
            }
            Ok(output) => {
                let msg = format!(
                    "Failed to delete namespace {} during recovery: {}",
                    ns_name, output.stderr
                );
                warn!("{}", msg);
                errors.push(msg);
            }
            Err(e) => {
                let msg = format!("Error deleting namespace {}: {}", ns_name, e);
                warn!("{}", msg);
                errors.push(msg);
            }
        }
    }

    if errors.is_empty() {
        info!("Basic recovery cleanup completed successfully");
    } else {
        warn!(
            "Basic recovery cleanup completed with {} error(s): {}",
            errors.len(),
            errors.join("; ")
        );
    }

    Ok(())
}

/// 清理 pidfile 和 statefile
fn cleanup_pidfile_state(pidfile_path: &str, statefile_path: &str) {
    remove_pidfile(pidfile_path);
    remove_state_file(statefile_path);
}

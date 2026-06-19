use anyhow::{anyhow, Result};
use colored::Colorize;
use log::{error, info, warn};
use std::process::Command;

/// 系统信息
#[derive(Debug)]
pub struct SystemInfo {
    pub os: String,
    pub kernel: String,
    pub cpu_cores: usize,
    pub memory_mb: u64,
    pub virtualization: String,
}

/// 检查系统环境
pub fn check_environment() -> Result<(SystemInfo, Vec<String>)> {
    info!("Checking system environment...");

    let system_info = collect_system_info()?;
    print_system_info(&system_info);

    check_iproute2()?;
    check_netns_support()?;
    check_permissions()?;
    let network_devices = scan_network_devices()?;

    info!("Environment check passed");
    Ok((system_info, network_devices))
}

/// 收集系统信息
fn collect_system_info() -> Result<SystemInfo> {
    // 获取操作系统信息
    let os = get_os_info()?;

    // 获取内核版本
    let kernel = get_kernel_version()?;

    // 获取 CPU 核心数
    let cpu_cores = get_cpu_cores();

    // 获取内存大小
    let memory_mb = get_memory_mb()?;

    // 检测虚拟化环境
    let virtualization = detect_virtualization()?;

    Ok(SystemInfo {
        os,
        kernel,
        cpu_cores,
        memory_mb,
        virtualization,
    })
}

/// 获取操作系统信息
fn get_os_info() -> Result<String> {
    let output = Command::new("uname").arg("-s").output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 获取内核版本
fn get_kernel_version() -> Result<String> {
    let output = Command::new("uname").arg("-r").output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 获取 CPU 核心数
/// 该函数始终返回有效值（无法获取时返回 1），因此不使用 Result
fn get_cpu_cores() -> usize {
    // 尝试使用 nproc（Linux）
    if let Ok(output) = Command::new("nproc").output() {
        if output.status.success() {
            let cores_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(cores) = cores_str.parse() {
                return cores;
            }
        }
    }

    // 尝试使用 sysctl（macOS）
    if let Ok(output) = Command::new("sysctl").args(["-n", "hw.ncpu"]).output() {
        if output.status.success() {
            let cores_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(cores) = cores_str.parse() {
                return cores;
            }
        }
    }

    // 默认返回 1
    1
}

/// 获取内存大小（MB）
fn get_memory_mb() -> Result<u64> {
    // 尝试使用 free（Linux）
    if let Ok(output) = Command::new("free").arg("-m").output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);

            // 解析 free 命令输出，获取总内存
            for line in output_str.lines() {
                if line.starts_with("Mem:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(mem) = parts[1].parse() {
                            return Ok(mem);
                        }
                    }
                }
            }
        }
    }

    // 尝试使用 sysctl（macOS）
    if let Ok(output) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
        if output.status.success() {
            let mem_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(mem_bytes) = mem_str.parse::<u64>() {
                return Ok(mem_bytes / 1024 / 1024); // 转换为 MB
            }
        }
    }

    // 默认返回 1024 MB
    Ok(1024)
}

/// 检测虚拟化环境
fn detect_virtualization() -> Result<String> {
    // 检查是否在 Docker 中
    if std::path::Path::new("/.dockerenv").exists() {
        return Ok("Docker".to_string());
    }

    // 检查是否在 LXC 中
    let output = Command::new("systemd-detect-virt").output();
    if let Ok(output) = output {
        let virt = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !virt.is_empty() && virt != "none" {
            return Ok(virt);
        }
    }

    // 检查 /proc/1/cgroup
    if let Ok(content) = std::fs::read_to_string("/proc/1/cgroup") {
        if content.contains("docker") || content.contains("kubepods") {
            return Ok("Docker".to_string());
        }
        if content.contains("lxc") {
            return Ok("LXC".to_string());
        }
    }

    Ok("Bare Metal".to_string())
}

/// 打印系统信息
fn print_system_info(info: &SystemInfo) {
    println!("{}", "=== System Information ===".green().bold());
    println!("OS: {}", info.os);
    println!("Kernel: {}", info.kernel);
    println!("CPU Cores: {}", info.cpu_cores);
    println!("Memory: {} MB", info.memory_mb);
    println!("Virtualization: {}", info.virtualization);
    println!();
}

/// 检查 iproute2 是否安装
fn check_iproute2() -> Result<()> {
    info!("Checking iproute2 installation...");

    // 检查是否在 macOS 上
    if cfg!(target_os = "macos") {
        warn!("Running on macOS, skipping iproute2 check (Linux only)");
        return Ok(());
    }

    // 检查 ip 命令
    let ip_exists = Command::new("which").arg("ip").output()?.status.success();
    if !ip_exists {
        error!("ip command not found. Please install iproute2.");
        return Err(anyhow!("iproute2 not installed"));
    }

    // 检查 iptables 命令
    let iptables_exists = Command::new("which")
        .arg("iptables")
        .output()?
        .status
        .success();
    if !iptables_exists {
        warn!("iptables not found. NAT configuration may not work.");
    }

    info!("iproute2 check passed");
    Ok(())
}

/// 检查 netns 支持
fn check_netns_support() -> Result<()> {
    info!("Checking netns support...");

    // 检查是否在 macOS 上
    if cfg!(target_os = "macos") {
        warn!("Running on macOS, skipping netns support check (Linux only)");
        return Ok(());
    }

    // 尝试创建测试命名空间
    let output = Command::new("ip")
        .args(["netns", "add", "test-netns-check"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Failed to create test namespace: {}", stderr);
        return Err(anyhow!("netns not supported or insufficient permissions"));
    }

    // 清理测试命名空间
    Command::new("ip")
        .args(["netns", "delete", "test-netns-check"])
        .output()?;

    info!("netns support check passed");
    Ok(())
}

/// 检查权限
fn check_permissions() -> Result<()> {
    info!("Checking permissions...");

    // 检查是否在 macOS 上
    if cfg!(target_os = "macos") {
        warn!("Running on macOS, skipping root permission check (Linux only)");
        return Ok(());
    }

    // 检查是否为 root 用户
    let uid = unsafe { libc::getuid() };
    if uid != 0 {
        error!("This program requires root privileges. Please run with sudo.");
        return Err(anyhow!("Insufficient permissions"));
    }

    info!("Permission check passed");
    Ok(())
}

/// 扫描网络设备
fn scan_network_devices() -> Result<Vec<String>> {
    info!("Scanning network devices...");

    let mut devices = Vec::new();

    // 检查是否在 macOS 上
    if cfg!(target_os = "macos") {
        // macOS 使用 ifconfig 或 networksetup
        if let Ok(output) = Command::new("ifconfig").args(["-l"]).output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                for device in output_str.split_whitespace() {
                    if device != "lo0" {
                        devices.push(device.to_string());
                    }
                }
            }
        }
    } else {
        // Linux 使用 ip link show
        if let Ok(output) = Command::new("ip").args(["link", "show"]).output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                for line in output_str.lines() {
                    if line.contains(": ") && !line.contains("lo:") {
                        if let Some(name) = line.split(": ").nth(1) {
                            let name = name.split('@').next().unwrap_or(name);
                            let name = name.trim();
                            if !name.is_empty() {
                                devices.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    info!("Found network devices: {:?}", devices);
    Ok(devices)
}

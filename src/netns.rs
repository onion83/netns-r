use anyhow::{Result, anyhow};
use log::{info, warn, debug};
use crate::command::{CommandExecutor, SystemCommandExecutor};
use crate::config::Config;

/// 管理网络命名空间
pub fn manage_namespaces(config: &Config) -> Result<()> {
    info!("Managing network namespaces...");
    
    let executor = SystemCommandExecutor::new(false);
    
    for netns_config in &config.netns_list {
        let ns_name = &netns_config.name;
        
        info!("Processing namespace: {}", ns_name);
        
        // 检查命名空间是否已存在
        if namespace_exists(ns_name, &executor)? {
            warn!("Namespace {} already exists, deleting...", ns_name);
            delete_namespace(ns_name, &executor)?;
        }
        
        // 创建命名空间
        create_namespace(ns_name, &executor)?;
        
        // 配置网络
        configure_network(ns_name, netns_config, config, &executor)?;
        
        // 配置 DNS
        configure_dns(ns_name, netns_config, config)?;
        
        info!("Namespace {} configured successfully", ns_name);
    }
    
    Ok(())
}

/// 检查命名空间是否存在
fn namespace_exists(name: &str, executor: &SystemCommandExecutor) -> Result<bool> {
    let output = executor.execute("ip", &["netns", "list"])?;
    let ns_list = output.stdout;
    
    // 检查输出中是否包含命名空间名称
    for line in ns_list.lines() {
        let ns_name = line.split_whitespace().next().unwrap_or("");
        if ns_name == name {
            return Ok(true);
        }
    }
    
    Ok(false)
}

/// 创建命名空间
fn create_namespace(name: &str, executor: &SystemCommandExecutor) -> Result<()> {
    info!("Creating namespace: {}", name);
    
    let output = executor.execute("ip", &["netns", "add", name])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!("Failed to create namespace {}: {}", name, output.stderr));
    }
    
    // 启用 loopback 接口
    executor.execute_in_ns(name, "ip", &["link", "set", "lo", "up"])?;
    
    info!("Namespace {} created successfully", name);
    Ok(())
}

/// 删除命名空间
fn delete_namespace(name: &str, executor: &SystemCommandExecutor) -> Result<()> {
    info!("Deleting namespace: {}", name);
    
    // 先杀死命名空间内的所有进程
    let pids_output = executor.execute("ip", &["netns", "pids", name])?;
    if !pids_output.stdout.trim().is_empty() {
        for pid in pids_output.stdout.lines() {
            if let Ok(pid) = pid.trim().parse::<i32>() {
                debug!("Killing process {} in namespace {}", pid, name);
                let _ = executor.execute("kill", &["-9", &pid.to_string()]);
            }
        }
    }
    
    // 删除命名空间
    let output = executor.execute("ip", &["netns", "delete", name])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!("Failed to delete namespace {}: {}", name, output.stderr));
    }
    
    info!("Namespace {} deleted successfully", name);
    Ok(())
}

/// 配置网络
fn configure_network(
    ns_name: &str,
    netns_config: &crate::config::NetnsConfig,
    global_config: &Config,
    executor: &SystemCommandExecutor,
) -> Result<()> {
    info!("Configuring network for namespace: {}", ns_name);
    
    let use_raw_dev = global_config.default.use_raw_dev;
    let network = &netns_config.network;
    
    if use_raw_dev {
        // 使用原始设备模式
        configure_raw_device(ns_name, network, executor)?;
    } else {
        // 使用 veth 设备模式
        configure_veth_device(ns_name, network, global_config, executor)?;
    }
    
    // 配置 IP 地址
    if let Some(ipv4_addr) = &network.ipv4_address {
        configure_ip_address(ns_name, ipv4_addr, netns_config, global_config, executor)?;
    }
    
    if let Some(ipv6_addr) = &network.ipv6_address {
        configure_ip_address(ns_name, ipv6_addr, netns_config, global_config, executor)?;
    }
    
    // 配置默认路由
    configure_default_route(ns_name, network, global_config, executor)?;
    
    info!("Network configured for namespace: {}", ns_name);
    Ok(())
}

/// 配置原始设备
fn configure_raw_device(
    ns_name: &str,
    network: &crate::config::NetnsNetworkConfig,
    executor: &SystemCommandExecutor,
) -> Result<()> {
    let raw_dev = network.raw_dev.as_ref().ok_or_else(|| anyhow!("raw-dev not specified"))?;
    
    info!("Configuring raw device {} for namespace {}", raw_dev, ns_name);
    
    // 将设备移动到命名空间
    let output = executor.execute("ip", &["link", "set", "dev", raw_dev, "netns", ns_name])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!("Failed to move device {} to namespace {}: {}", raw_dev, ns_name, output.stderr));
    }
    
    // 在命名空间内启用设备
    executor.execute_in_ns(ns_name, "ip", &["link", "set", raw_dev, "up"])?;
    
    Ok(())
}

/// 配置 veth 设备
fn configure_veth_device(
    ns_name: &str,
    network: &crate::config::NetnsNetworkConfig,
    global_config: &Config,
    executor: &SystemCommandExecutor,
) -> Result<()> {
    let veth_dev = network.veth_dev.as_ref().ok_or_else(|| anyhow!("veth-dev not specified"))?;
    let veth_peer = format!("{}-peer", veth_dev);
    
    info!("Configuring veth device {} for namespace {}", veth_dev, ns_name);
    
    // 创建 veth 设备对
    let output = executor.execute("ip", &[
        "link", "add", veth_dev, "type", "veth", "peer", "name", &veth_peer,
    ])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!("Failed to create veth device {}: {}", veth_dev, output.stderr));
    }
    
    // 将 veth 设备移动到命名空间
    let output = executor.execute("ip", &["link", "set", veth_dev, "netns", ns_name])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!("Failed to move veth device to namespace {}: {}", ns_name, output.stderr));
    }
    
    // 在命名空间内启用 veth 设备
    executor.execute_in_ns(ns_name, "ip", &["link", "set", veth_dev, "up"])?;
    
    // 在主机端启用 peer 设备
    executor.execute("ip", &["link", "set", &veth_peer, "up"])?;
    
    // 配置 NAT（如果需要）
    configure_nat(ns_name, veth_dev, &veth_peer, network, global_config, executor)?;
    
    Ok(())
}

/// 配置 NAT
/// outbound-dev 优先级：netns 内 outbound-dev > default.outbound-dev > 不配置
fn configure_nat(
    ns_name: &str,
    _veth_dev: &str,
    _veth_peer: &str,
    network: &crate::config::NetnsNetworkConfig,
    global_config: &Config,
    executor: &SystemCommandExecutor,
) -> Result<()> {
    // 获取出口设备：netns 配置 > default 配置
    let outbound_dev = network.outbound_dev.as_ref()
        .or(global_config.default.outbound_dev.as_ref());
    
    let outbound_dev = match outbound_dev {
        Some(dev) => dev,
        None => {
            info!("No outbound device configured for namespace {}, skipping NAT", ns_name);
            return Ok(());
        }
    };
    
    info!("Configuring NAT for namespace {} via {}", ns_name, outbound_dev);
    
    // 获取 IPv4 网关
    let ipv4_gateway = network.ipv4_gateway.as_ref()
        .or(global_config.default.network.ipv4_gateway.as_ref())
        .ok_or_else(|| anyhow!("IPv4 gateway not specified"))?;
    
    // 在主机端配置 IPv4 NAT（MASQUERADE）
    let output = executor.execute("iptables", &[
        "-t", "nat", "-A", "POSTROUTING", "-s", ipv4_gateway, "-o", outbound_dev, "-j", "MASQUERADE",
    ])?;
    if output.status != crate::command::ExecStatus::Success {
        warn!("Failed to configure IPv4 NAT: {}", output.stderr);
    }
    
    // 启用 IP 转发
    executor.execute("sysctl", &["-w", "net.ipv4.ip_forward=1"])?;
    
    Ok(())
}

/// 配置 IP 地址
fn configure_ip_address(
    ns_name: &str,
    address: &str,
    netns_config: &crate::config::NetnsConfig,
    global_config: &Config,
    executor: &SystemCommandExecutor,
) -> Result<()> {
    info!("Configuring IP address {} for namespace {}", address, ns_name);
    
    // 获取设备名称
    let dev = if global_config.default.use_raw_dev {
        netns_config.network.raw_dev.as_ref().ok_or_else(|| anyhow!("raw-dev not specified"))?
    } else {
        netns_config.network.veth_dev.as_ref().ok_or_else(|| anyhow!("veth-dev not specified"))?
    };
    
    let output = executor.execute_in_ns(ns_name, "ip", &["addr", "add", address, "dev", dev])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!("Failed to configure IP address {}: {}", address, output.stderr));
    }
    
    Ok(())
}

/// 配置默认路由
fn configure_default_route(
    ns_name: &str,
    network: &crate::config::NetnsNetworkConfig,
    global_config: &Config,
    executor: &SystemCommandExecutor,
) -> Result<()> {
    // 配置 IPv4 默认路由
    if let Some(gateway) = network.ipv4_gateway.as_ref().or(global_config.default.network.ipv4_gateway.as_ref()) {
        info!("Configuring IPv4 default route via {} for namespace {}", gateway, ns_name);
        
        let output = executor.execute_in_ns(ns_name, "ip", &["route", "add", "default", "via", gateway])?;
        if output.status != crate::command::ExecStatus::Success {
            warn!("Failed to configure IPv4 default route: {}", output.stderr);
        }
    }
    
    // 配置 IPv6 默认路由
    if let Some(gateway) = network.ipv6_gateway.as_ref().or(global_config.default.network.ipv6_gateway.as_ref()) {
        info!("Configuring IPv6 default route via {} for namespace {}", gateway, ns_name);
        
        let output = executor.execute_in_ns(ns_name, "ip", &["-6", "route", "add", "default", "via", gateway])?;
        if output.status != crate::command::ExecStatus::Success {
            warn!("Failed to configure IPv6 default route: {}", output.stderr);
        }
    }
    
    Ok(())
}

/// 配置 DNS
/// 优先级：netns 内 dns > default.network.dns-servers > 不配置
fn configure_dns(
    ns_name: &str,
    netns_config: &crate::config::NetnsConfig,
    global_config: &Config,
) -> Result<()> {
    // 优先使用 netns 容器内的 dns-servers
    let dns_servers = if let Some(servers) = &netns_config.network.dns_servers {
        if !servers.is_empty() {
            Some(servers)
        } else {
            None
        }
    } else {
        // 否则使用 default.network.dns-servers
        global_config.default.network.dns_servers.as_ref()
    };
    
    let netns_dir = format!("/etc/netns/{}", ns_name);
    let resolv_conf = format!("{}/resolv.conf", netns_dir);
    
    match dns_servers {
        Some(servers) if !servers.is_empty() => {
            info!("Configuring DNS for namespace {} with servers: {:?}", ns_name, servers);
            
            // 创建 /etc/netns 目录
            std::fs::create_dir_all(&netns_dir)?;
            
            // 创建 resolv.conf 文件
            let mut content = String::new();
            for server in servers {
                content.push_str(&format!("nameserver {}\n", server));
            }
            // 追加 DNS 优化选项，避免超时卡死
            content.push_str("options timeout:1 attempts:1 rotate single-request-reopen\n");
            std::fs::write(&resolv_conf, content)?;
            
            info!("DNS configured for namespace {}", ns_name);
        }
        _ => {
            // 没有配置 DNS 服务器，删除已有的 resolv.conf（如果存在）
            if std::path::Path::new(&resolv_conf).exists() {
                info!("No DNS servers configured for namespace {}, removing resolv.conf", ns_name);
                let _ = std::fs::remove_file(&resolv_conf);
            }
            // 如果 /etc/netns/{ns} 目录为空，也删除它
            if std::path::Path::new(&netns_dir).exists() {
                if let Ok(entries) = std::fs::read_dir(&netns_dir) {
                    if entries.count() == 0 {
                        let _ = std::fs::remove_dir(&netns_dir);
                    }
                }
            }
        }
    }
    
    Ok(())
}

/// 清理所有网络命名空间及相关资源
/// 供 --clean 和信号处理共用
pub fn cleanup_namespaces(config: &Config) -> Result<()> {
    let executor = SystemCommandExecutor::new(false);
    
    for netns_config in &config.netns_list {
        let ns_name = &netns_config.name;
        info!("Cleaning up namespace: {}", ns_name);
        
        // 1. 先杀死命名空间内的所有进程
        let pids_output = executor.execute("ip", &["netns", "pids", ns_name])?;
        if !pids_output.stdout.trim().is_empty() {
            for pid in pids_output.stdout.lines() {
                if let Ok(pid) = pid.trim().parse::<i32>() {
                    debug!("Killing process {} in namespace {}", pid, ns_name);
                    let _ = executor.execute("kill", &["-9", &pid.to_string()]);
                }
            }
        }
        
        // 2. 删除命名空间（会自动删除命名空间内的 veth 设备）
        let output = executor.execute("ip", &["netns", "delete", ns_name])?;
        if output.status == crate::command::ExecStatus::Success {
            info!("Namespace {} deleted", ns_name);
        } else {
            warn!("Failed to delete namespace {}: {}", ns_name, output.stderr);
        }
        
        // 3. 清理 veth peer 设备（主机端）
        if !config.default.use_raw_dev {
            if let Some(veth_dev) = &netns_config.network.veth_dev {
                let peer_name = format!("{}-peer", veth_dev);
                info!("Deleting veth peer: {}", peer_name);
                let _ = executor.execute("ip", &["link", "delete", &peer_name]);
            }
        }
        
        // 4. 清理 iptables NAT 规则
        if let Some(gateway) = netns_config.network.ipv4_gateway.as_ref()
            .or(config.default.network.ipv4_gateway.as_ref()) 
        {
            // 获取出口设备：netns 配置 > default 配置
            let outbound_dev = netns_config.network.outbound_dev.as_ref()
                .or(config.default.outbound_dev.as_ref());
            
            if let Some(dev) = outbound_dev {
                info!("Cleaning iptables NAT rule for {} via {}", gateway, dev);
                let _ = executor.execute("iptables", &[
                    "-t", "nat", "-D", "POSTROUTING", "-s", gateway, "-o", dev, "-j", "MASQUERADE",
                ]);
            }
        }
        
        // 5. 清理 DNS 配置目录
        let dns_dir = format!("/etc/netns/{}", ns_name);
        if std::path::Path::new(&dns_dir).exists() {
            info!("Removing DNS config directory: {}", dns_dir);
            let _ = std::fs::remove_dir_all(&dns_dir);
        }
    }
    
    info!("Cleanup completed");
    Ok(())
}
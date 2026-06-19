use crate::command::{CommandExecutor, SystemCommandExecutor};
use crate::config::Config;
use anyhow::{anyhow, Result};
use log::{debug, info, warn};

/// 管理网络命名空间
pub fn manage_namespaces(config: &Config, dry_run: bool) -> Result<()> {
    info!("Managing network namespaces...");

    let executor = SystemCommandExecutor::new(dry_run);

    for netns_config in &config.netns_list {
        let ns_name = &netns_config.name;

        info!("Processing namespace: {}", ns_name);

        // 检查命名空间是否已存在
        if namespace_exists(ns_name, &executor)? {
            warn!("Namespace {} already exists, deleting...", ns_name);
            // raw-dev 模式下，先将 raw-dev 移回主机命名空间，避免随命名空间删除而丢失
            if config.default.use_raw_dev {
                if let Some(raw_dev) = &netns_config.network.raw_dev {
                    move_raw_dev_to_host(ns_name, raw_dev, &executor);
                }
            }
            delete_namespace(ns_name, &executor)?;
        }

        // 创建命名空间
        create_namespace(ns_name, &executor)?;

        // 配置网络
        configure_network(ns_name, netns_config, config, &executor)?;

        // 配置 DNS
        configure_dns(ns_name, netns_config, config, dry_run)?;

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
        return Err(anyhow!(
            "Failed to create namespace {}: {}",
            name,
            output.stderr
        ));
    }

    // 启用 loopback 接口
    executor.execute_in_ns(name, "ip", &["link", "set", "lo", "up"])?;

    info!("Namespace {} created successfully", name);
    Ok(())
}

/// 删除命名空间
/// 先 SIGKILL 命名空间内所有进程，再删除命名空间
/// ip netns delete 只删除 bind mount，不会自动杀进程，需手动清理
fn delete_namespace(name: &str, executor: &SystemCommandExecutor) -> Result<()> {
    info!("Deleting namespace: {}", name);

    // SIGKILL 命名空间内所有进程（直接杀，不等待优雅退出）
    kill_processes_in_namespace(name, executor);

    let output = executor.execute("ip", &["netns", "delete", name])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!(
            "Failed to delete namespace {}: {}",
            name,
            output.stderr
        ));
    }

    info!("Namespace {} deleted successfully", name);
    Ok(())
}

/// 杀死命名空间内的所有进程（SIGKILL）
fn kill_processes_in_namespace(ns_name: &str, executor: &SystemCommandExecutor) {
    if let Ok(pids_output) = executor.execute("ip", &["netns", "pids", ns_name]) {
        for pid_line in pids_output.stdout.lines() {
            if let Ok(pid) = pid_line.trim().parse::<i32>() {
                debug!("Killing process {} in namespace {}", pid, ns_name);
                let _ = executor.execute("kill", &["-9", &pid.to_string()]);
            }
        }
    }
}

/// 将 raw-dev 从命名空间移回主机命名空间（init netns, PID 1）
/// 在删除命名空间前调用，避免 raw-dev 随命名空间删除而丢失
fn move_raw_dev_to_host(ns_name: &str, raw_dev: &str, executor: &SystemCommandExecutor) {
    info!(
        "Moving raw-dev {} from namespace {} back to host",
        raw_dev, ns_name
    );
    match executor.execute_in_ns(ns_name, "ip", &["link", "set", raw_dev, "netns", "1"]) {
        Ok(output) if output.status == crate::command::ExecStatus::Success => {
            info!("raw-dev {} moved back to host", raw_dev);
        }
        Ok(output) => {
            warn!(
                "Failed to move raw-dev {} back to host: {}",
                raw_dev, output.stderr
            );
        }
        Err(e) => {
            warn!("Failed to move raw-dev {} back to host: {}", raw_dev, e);
        }
    }
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
    let raw_dev = network
        .raw_dev
        .as_ref()
        .ok_or_else(|| anyhow!("raw-dev not specified"))?;

    info!(
        "Configuring raw device {} for namespace {}",
        raw_dev, ns_name
    );

    // 将设备移动到命名空间
    let output = executor.execute("ip", &["link", "set", "dev", raw_dev, "netns", ns_name])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!(
            "Failed to move device {} to namespace {}: {}",
            raw_dev,
            ns_name,
            output.stderr
        ));
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
    let veth_dev = network
        .veth_dev
        .as_ref()
        .ok_or_else(|| anyhow!("veth-dev not specified"))?;
    let veth_peer = format!("{}-peer", veth_dev);

    info!(
        "Configuring veth device {} for namespace {}",
        veth_dev, ns_name
    );

    // 创建 veth 设备对
    let output = executor.execute(
        "ip",
        &[
            "link", "add", veth_dev, "type", "veth", "peer", "name", &veth_peer,
        ],
    )?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!(
            "Failed to create veth device {}: {}",
            veth_dev,
            output.stderr
        ));
    }

    // 将 veth 设备移动到命名空间
    let output = executor.execute("ip", &["link", "set", veth_dev, "netns", ns_name])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!(
            "Failed to move veth device to namespace {}: {}",
            ns_name,
            output.stderr
        ));
    }

    // 在命名空间内启用 veth 设备
    executor.execute_in_ns(ns_name, "ip", &["link", "set", veth_dev, "up"])?;

    // 在主机端启用 peer 设备
    executor.execute("ip", &["link", "set", &veth_peer, "up"])?;

    // 给主机端 veth-peer 配置网关 IP（veth 模式下 veth-peer 充当命名空间的网关）
    // 网关地址 + 从命名空间 ipv4-address 提取的子网前缀
    configure_veth_peer_gateway(ns_name, &veth_peer, network, global_config, executor)?;

    // 配置 NAT（如果需要）
    configure_nat(
        ns_name,
        veth_dev,
        &veth_peer,
        network,
        global_config,
        executor,
    )?;

    Ok(())
}

/// 给主机端 veth-peer 配置网关 IP 地址
/// veth 模式下，主机端 veth-peer 充当命名空间内的网关，必须配置与命名空间同网段的 IP
fn configure_veth_peer_gateway(
    ns_name: &str,
    veth_peer: &str,
    network: &crate::config::NetnsNetworkConfig,
    global_config: &Config,
    executor: &SystemCommandExecutor,
) -> Result<()> {
    // IPv4：网关 + 从 ipv4-address 提取的前缀
    let ipv4_gateway = network
        .ipv4_gateway
        .as_ref()
        .or(global_config.default.network.ipv4_gateway.as_ref());
    if let Some(gateway) = ipv4_gateway {
        if let Some(prefix) = network.ipv4_address.as_ref().and_then(|a| a.split('/').nth(1)) {
            let peer_addr = format!("{}/{}", gateway, prefix);
            info!(
                "Configuring veth-peer {} with IPv4 gateway IP {} for namespace {}",
                veth_peer, peer_addr, ns_name
            );
            let output =
                executor.execute("ip", &["addr", "add", &peer_addr, "dev", veth_peer])?;
            if output.status != crate::command::ExecStatus::Success {
                // 地址可能已存在（重复运行），warn 但不中断
                warn!(
                    "Failed to configure veth-peer IPv4 gateway IP: {}",
                    output.stderr
                );
            }
        }
    }

    // IPv6：网关 + 从 ipv6-address 提取的前缀
    let ipv6_gateway = network
        .ipv6_gateway
        .as_ref()
        .or(global_config.default.network.ipv6_gateway.as_ref());
    if let Some(gateway) = ipv6_gateway {
        if let Some(prefix) = network.ipv6_address.as_ref().and_then(|a| a.split('/').nth(1)) {
            let peer_addr = format!("{}/{}", gateway, prefix);
            info!(
                "Configuring veth-peer {} with IPv6 gateway IP {} for namespace {}",
                veth_peer, peer_addr, ns_name
            );
            let output = executor.execute(
                "ip",
                &["-6", "addr", "add", &peer_addr, "dev", veth_peer],
            )?;
            if output.status != crate::command::ExecStatus::Success {
                warn!(
                    "Failed to configure veth-peer IPv6 gateway IP: {}",
                    output.stderr
                );
            }
        }
    }

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
    let outbound_dev = network
        .outbound_dev
        .as_ref()
        .or(global_config.default.outbound_dev.as_ref());

    let outbound_dev = match outbound_dev {
        Some(dev) => dev,
        None => {
            info!(
                "No outbound device configured for namespace {}, skipping NAT",
                ns_name
            );
            return Ok(());
        }
    };

    info!(
        "Configuring NAT for namespace {} via {}",
        ns_name, outbound_dev
    );

    // 获取命名空间内的 IPv4 地址（含 CIDR），用作 MASQUERADE 的源地址
    // iptables 会自动将 IP/CIDR 掩码为网络地址，例如 192.168.11.241/24 -> 192.168.11.0/24
    let ipv4_source = network.ipv4_address.as_ref().ok_or_else(|| {
        anyhow!(
            "IPv4 address not specified, cannot configure NAT for namespace {}",
            ns_name
        )
    })?;

    // 在主机端配置 IPv4 NAT（MASQUERADE）
    let output = executor.execute(
        "iptables",
        &[
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-s",
            ipv4_source,
            "-o",
            outbound_dev,
            "-j",
            "MASQUERADE",
        ],
    )?;
    if output.status != crate::command::ExecStatus::Success {
        warn!("Failed to configure IPv4 NAT: {}", output.stderr);
    }

    // 启用 IPv4 转发
    executor.execute("sysctl", &["-w", "net.ipv4.ip_forward=1"])?;

    // 配置 IPv6 NAT（如果命名空间配置了 IPv6 地址）
    if let Some(ipv6_source) = &network.ipv6_address {
        info!(
            "Configuring IPv6 NAT for namespace {} via {}",
            ns_name, outbound_dev
        );

        // 在主机端配置 IPv6 NAT（MASQUERADE）
        let output = executor.execute(
            "ip6tables",
            &[
                "-t",
                "nat",
                "-A",
                "POSTROUTING",
                "-s",
                ipv6_source,
                "-o",
                outbound_dev,
                "-j",
                "MASQUERADE",
            ],
        )?;
        if output.status != crate::command::ExecStatus::Success {
            warn!("Failed to configure IPv6 NAT: {}", output.stderr);
        }

        // 启用 IPv6 转发
        executor.execute(
            "sysctl",
            &["-w", "net.ipv6.conf.all.forwarding=1"],
        )?;
    }

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
    info!(
        "Configuring IP address {} for namespace {}",
        address, ns_name
    );

    // 获取设备名称
    let dev = if global_config.default.use_raw_dev {
        netns_config
            .network
            .raw_dev
            .as_ref()
            .ok_or_else(|| anyhow!("raw-dev not specified"))?
    } else {
        netns_config
            .network
            .veth_dev
            .as_ref()
            .ok_or_else(|| anyhow!("veth-dev not specified"))?
    };

    let output = executor.execute_in_ns(ns_name, "ip", &["addr", "add", address, "dev", dev])?;
    if output.status != crate::command::ExecStatus::Success {
        return Err(anyhow!(
            "Failed to configure IP address {}: {}",
            address,
            output.stderr
        ));
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
    if let Some(gateway) = network.ipv4_gateway.as_ref().or(global_config
        .default
        .network
        .ipv4_gateway
        .as_ref())
    {
        info!(
            "Configuring IPv4 default route via {} for namespace {}",
            gateway, ns_name
        );

        let output =
            executor.execute_in_ns(ns_name, "ip", &["route", "add", "default", "via", gateway])?;
        if output.status != crate::command::ExecStatus::Success {
            return Err(anyhow!(
                "Failed to configure IPv4 default route via {} for namespace {}: {}. \
                 Hint: gateway must be in the same subnet as the namespace's ipv4-address (veth mode) \
                 or directly reachable (raw-dev mode).",
                gateway,
                ns_name,
                output.stderr
            ));
        }
    }

    // 配置 IPv6 默认路由（仅当命名空间配置了 IPv6 地址时才添加）
    if network.ipv6_address.is_some() {
        if let Some(gateway) = network.ipv6_gateway.as_ref().or(global_config
            .default
            .network
            .ipv6_gateway
            .as_ref())
        {
            info!(
                "Configuring IPv6 default route via {} for namespace {}",
                gateway, ns_name
            );

            let output = executor.execute_in_ns(
                ns_name,
                "ip",
                &["-6", "route", "add", "default", "via", gateway],
            )?;
            if output.status != crate::command::ExecStatus::Success {
                return Err(anyhow!(
                    "Failed to configure IPv6 default route via {} for namespace {}: {}. \
                     Hint: gateway must be in the same subnet as the namespace's ipv6-address (veth mode) \
                     or directly reachable (raw-dev mode).",
                    gateway,
                    ns_name,
                    output.stderr
                ));
            }
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
    dry_run: bool,
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
            info!(
                "Configuring DNS for namespace {} with servers: {:?}",
                ns_name, servers
            );

            if dry_run {
                info!("[DRY RUN] mkdir -p {}", netns_dir);
                info!(
                    "[DRY RUN] write {} nameservers to {}",
                    servers.len(),
                    resolv_conf
                );
                return Ok(());
            }

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
                info!(
                    "No DNS servers configured for namespace {}, removing resolv.conf",
                    ns_name
                );
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
    let mut errors: Vec<String> = Vec::new();

    for netns_config in &config.netns_list {
        let ns_name = &netns_config.name;
        info!("Cleaning up namespace: {}", ns_name);

        // 1. raw-dev 模式下，先将 raw-dev 移回主机命名空间，避免随命名空间删除而丢失
        if config.default.use_raw_dev {
            if let Some(raw_dev) = &netns_config.network.raw_dev {
                move_raw_dev_to_host(ns_name, raw_dev, &executor);
            }
        }

        // 2. SIGKILL 命名空间内所有进程，然后删除命名空间
        // ip netns delete 只删除 bind mount，不会自动杀进程，需手动清理
        kill_processes_in_namespace(ns_name, &executor);

        // 3. 删除命名空间（会自动删除命名空间内的 veth 设备）
        // 即使删除失败，也继续清理主机端资源（veth-peer、iptables、DNS）
        match executor.execute("ip", &["netns", "delete", ns_name]) {
            Ok(output) if output.status == crate::command::ExecStatus::Success => {
                info!("Namespace {} deleted", ns_name);
            }
            Ok(output) => {
                let msg = format!("Failed to delete namespace {}: {}", ns_name, output.stderr);
                warn!("{}", msg);
                errors.push(msg);
            }
            Err(e) => {
                debug!("Namespace {} already deleted or error: {}", ns_name, e);
            }
        }

        // 3. 清理 veth peer 设备（主机端，仅 veth 模式）
        if !config.default.use_raw_dev {
            if let Some(veth_dev) = &netns_config.network.veth_dev {
                let peer_name = format!("{}-peer", veth_dev);
                info!("Deleting veth peer: {}", peer_name);
                let _ = executor.execute("ip", &["link", "delete", &peer_name]);
            }
        }

        // 4. 清理 iptables/ip6tables NAT 规则（仅 veth 模式，raw-dev 模式不配置 NAT）
        if !config.default.use_raw_dev {
            let outbound_dev = netns_config
                .network
                .outbound_dev
                .as_ref()
                .or(config.default.outbound_dev.as_ref());

            if let Some(dev) = outbound_dev {
                // IPv4 NAT 清理
                if let Some(ipv4_source) = &netns_config.network.ipv4_address {
                    info!("Cleaning iptables NAT rule for {} via {}", ipv4_source, dev);
                    let _ = executor.execute(
                        "iptables",
                        &[
                            "-t",
                            "nat",
                            "-D",
                            "POSTROUTING",
                            "-s",
                            ipv4_source,
                            "-o",
                            dev,
                            "-j",
                            "MASQUERADE",
                        ],
                    );
                }

                // IPv6 NAT 清理
                if let Some(ipv6_source) = &netns_config.network.ipv6_address {
                    info!(
                        "Cleaning ip6tables NAT rule for {} via {}",
                        ipv6_source, dev
                    );
                    let _ = executor.execute(
                        "ip6tables",
                        &[
                            "-t",
                            "nat",
                            "-D",
                            "POSTROUTING",
                            "-s",
                            ipv6_source,
                            "-o",
                            dev,
                            "-j",
                            "MASQUERADE",
                        ],
                    );
                }
            }
        }

        // 5. 清理 DNS 配置目录
        let dns_dir = format!("/etc/netns/{}", ns_name);
        if std::path::Path::new(&dns_dir).exists() {
            info!("Removing DNS config directory: {}", dns_dir);
            let _ = std::fs::remove_dir_all(&dns_dir);
        }
    }

    if errors.is_empty() {
        info!("Cleanup completed");
        Ok(())
    } else {
        Err(anyhow!(
            "Cleanup completed with {} error(s): {}",
            errors.len(),
            errors.join("; ")
        ))
    }
}

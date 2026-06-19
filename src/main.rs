use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use log::{debug, info, warn};
use nix::sys::signal::{self, Signal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod config;
mod env_check;
mod command;
mod netns;
mod supervisor;
mod log_system;

/// netns-r: Network namespace management tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "netns-r.yaml")]
    config: String,

    /// Log level (debug, info, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Dry run mode (don't execute commands)
    #[arg(long)]
    dry_run: bool,

    /// Clean mode: delete all namespaces and related resources from config
    #[arg(long = "clean")]
    clean: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    // 初始化日志系统
    log_system::init(&args.log_level)?;
    
    info!("Starting netns-r");
    debug!("Arguments: {:?}", args);
    
    // 环境自检
    let (_system_info, network_devices) = env_check::check_environment()?;
    debug!("Network devices: {:?}", network_devices);
    
    // 加载配置
    let config = config::load_config(&args.config)?;
    debug!("Configuration loaded: {:?}", config);
    
    // 验证配置
    config::validate_config(&config)?;
    info!("Configuration validated");
    
    // 创建 stop flag 用于信号处理（在任何操作前创建）
    let stop_flag = Arc::new(AtomicBool::new(false));
    
    // 注册信号处理器（在任何操作前注册，确保能捕获 SIGTERM/SIGINT/SIGHUP）
    register_signal_handlers(stop_flag.clone())?;
    
    // 执行主逻辑
    if args.clean {
        info!("Clean mode - deleting all namespaces and resources");
        netns::cleanup_namespaces(&config)?;
    } else if args.dry_run {
        info!("Dry run mode - not executing commands");
        // 显示将要执行的命令
        show_dry_run_commands(&config);
    } else {
        // 检查是否收到信号
        if stop_flag.load(Ordering::Relaxed) || supervisor::SUPERVISOR_STOP_FLAG.load(Ordering::Relaxed) {
            warn!("Received stop signal before starting, exiting");
            return Ok(());
        }
        
        // 创建和管理网络命名空间
        netns::manage_namespaces(&config)?;
        
        // 如果启用了 supervisord，使用 supervisor 异步启动服务
        if config.global.supervisord.enable {
            // 启动 supervisor（supervisor 会异步启动所有服务）
            supervisor::start_supervisor(&config, stop_flag.clone())?;
            
            // supervisor 退出后清理网络命名空间
            netns::cleanup_namespaces(&config)?;
        } else {
            // oneshot 模式：启动服务但不监控
            supervisor::start_services_oneshot(&config)?;
        }
    }
    
    info!("netns-r completed successfully");
    Ok(())
}

/// 注册信号处理器
fn register_signal_handlers(stop_flag: Arc<AtomicBool>) -> Result<()> {
    // 使用 ctrlc 处理 SIGINT 和 SIGTERM
    let flag = stop_flag.clone();
    ctrlc::set_handler(move || {
        warn!("Received SIGINT/SIGTERM signal");
        flag.store(true, Ordering::Relaxed);
        supervisor::set_supervisor_stop_flag();
    })?;
    
    // 使用 nix 处理 SIGHUP（OpenRC reload 信号）
    extern "C" fn handle_sighup(_sig: i32) {
        warn!("Received SIGHUP signal");
        supervisor::set_supervisor_stop_flag();
    }
    let sa = signal::SigAction::new(
        signal::SigHandler::Handler(handle_sighup),
        signal::SaFlags::SA_RESTART,
        signal::SigSet::empty(),
    );
    unsafe {
        signal::sigaction(Signal::SIGHUP, &sa)?;
    }
    
    info!("Signal handler registered for SIGINT/SIGTERM/SIGHUP");
    Ok(())
}



/// 显示 dry-run 模式下的命令
fn show_dry_run_commands(config: &config::Config) {
    println!("{}", "=== Dry Run Mode - Commands that would be executed ===".yellow().bold());
    
    for netns_config in &config.netns_list {
        let ns_name = &netns_config.name;
        
        println!("\n{}", format!("--- Namespace: {} ---", ns_name).cyan().bold());
        
        // 显示创建命名空间的命令
        println!("ip netns add {}", ns_name);
        println!("ip netns exec {} ip link set lo up", ns_name);
        
        // 显示网络配置命令
        if config.default.use_raw_dev {
            if let Some(raw_dev) = &netns_config.network.raw_dev {
                println!("ip link set dev {} netns {}", raw_dev, ns_name);
                println!("ip netns exec {} ip link set {} up", ns_name, raw_dev);
            }
        } else {
            if let Some(veth_dev) = &netns_config.network.veth_dev {
                let veth_peer = format!("{}-peer", veth_dev);
                println!("ip link add {} type veth peer name {}", veth_dev, veth_peer);
                println!("ip link set {} netns {}", veth_dev, ns_name);
                println!("ip netns exec {} ip link set {} up", ns_name, veth_dev);
                println!("ip link set {} up", veth_peer);
            }
        }
        
        // 显示 IP 地址配置命令
        if let Some(ipv4_addr) = &netns_config.network.ipv4_address {
            let dev = if config.default.use_raw_dev {
                netns_config.network.raw_dev.as_deref().unwrap_or("eth0")
            } else {
                netns_config.network.veth_dev.as_deref().unwrap_or("eth0")
            };
            println!("ip netns exec {} ip addr add {} dev {}", ns_name, ipv4_addr, dev);
        }
        
        if let Some(ipv6_addr) = &netns_config.network.ipv6_address {
            let dev = if config.default.use_raw_dev {
                netns_config.network.raw_dev.as_deref().unwrap_or("eth0")
            } else {
                netns_config.network.veth_dev.as_deref().unwrap_or("eth0")
            };
            println!("ip netns exec {} ip -6 addr add {} dev {}", ns_name, ipv6_addr, dev);
        }
        
        // 显示路由配置命令
        if let Some(gateway) = netns_config.network.ipv4_gateway.as_ref().or(config.default.network.ipv4_gateway.as_ref()) {
            println!("ip netns exec {} ip route add default via {}", ns_name, gateway);
        }
        
        if let Some(gateway) = netns_config.network.ipv6_gateway.as_ref().or(config.default.network.ipv6_gateway.as_ref()) {
            println!("ip netns exec {} ip -6 route add default via {}", ns_name, gateway);
        }
        
        // 显示 DNS 配置命令
        if let Some(dns_servers) = &netns_config.network.dns_servers {
            if !dns_servers.is_empty() {
                println!("mkdir -p /etc/netns/{}", ns_name);
                for server in dns_servers {
                    println!("echo 'nameserver {}' >> /etc/netns/{}/resolv.conf", server, ns_name);
                }
                println!("echo 'options timeout:1 attempts:1 rotate single-request-reopen' >> /etc/netns/{}/resolv.conf", ns_name);
            }
        }
        
        // 显示服务启动命令
        let services = config::get_service_list(netns_config, config);
        for service_config in services {
            let command = config::replace_variables(&service_config.command, ns_name, &service_config.name);
            println!("ip netns exec {} sh -c '{}'", ns_name, command);
        }
    }
    
    println!("\n{}", "=== End of Dry Run ===".yellow().bold());
}
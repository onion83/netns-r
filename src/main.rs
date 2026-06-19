use anyhow::Result;
use clap::Parser;
use log::{debug, info, warn};
use nix::sys::signal::{self, Signal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod command;
mod config;
mod env_check;
mod log_system;
mod netns;
mod supervisor;

/// netns-r: Network namespace management tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "netns-r.yaml")]
    config: String,

    /// Log level (debug, info, error). Overrides config if specified.
    #[arg(short, long)]
    log_level: Option<String>,

    /// Dry run mode (don't execute commands)
    #[arg(long)]
    dry_run: bool,

    /// Clean mode: delete all namespaces and related resources from config
    #[arg(long = "clean")]
    clean: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // 加载配置（在日志初始化之前，以便使用配置中的日志路径）
    let config = config::load_config(&args.config)?;

    // 确定日志级别：CLI 参数优先，否则使用配置文件中的级别
    let log_level = args
        .log_level
        .as_deref()
        .unwrap_or(&config.global.log.level);

    // 初始化日志系统（使用配置中的日志路径，不再硬编码）
    log_system::init(log_level, &config.global.log.path)?;

    info!("Starting netns-r");
    debug!("Arguments: {:?}", args);
    debug!("Configuration loaded: {:?}", config);

    // 环境自检
    let (_system_info, network_devices) = env_check::check_environment()?;
    debug!("Network devices: {:?}", network_devices);

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
        // 通过 executor 的 dry_run 模式显示网络配置命令（单一代码路径，避免与真实逻辑漂移）
        netns::manage_namespaces(&config, true)?;
        // 显示服务启动命令
        show_dry_run_services(&config);
    } else {
        // 检查是否收到信号
        if stop_flag.load(Ordering::Relaxed)
            || supervisor::SUPERVISOR_STOP_FLAG.load(Ordering::Relaxed)
        {
            warn!("Received stop signal before starting, exiting");
            return Ok(());
        }

        // 创建和管理网络命名空间
        netns::manage_namespaces(&config, false)?;

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

/// 显示 dry-run 模式下的服务启动命令
/// 网络配置命令已由 manage_namespaces(dry_run=true) 通过 executor 统一输出
fn show_dry_run_services(config: &config::Config) {
    for netns_config in &config.netns_list {
        let ns_name = &netns_config.name;
        let services = config::get_service_list(netns_config, config);
        for service_config in services {
            let command =
                config::replace_variables(&service_config.command, ns_name, &service_config.name);
            info!("[DRY RUN] ip netns exec {} sh -c '{}'", ns_name, command);
        }
    }
}

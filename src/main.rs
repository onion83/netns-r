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
mod pidfile;
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

    /// PID file path (for OpenRC integration and crash recovery)
    #[arg(long, default_value = pidfile::DEFAULT_PIDFILE)]
    pidfile: String,

    /// State file path (tracks created namespaces for crash recovery)
    #[arg(long, default_value = pidfile::DEFAULT_STATE_FILE)]
    statefile: String,
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

    // 检查是否有上次异常退出的残留状态（SIGKILL 或崩溃后恢复）
    if args.clean {
        // clean 模式：强制清理所有命名空间 + pidfile/state
        info!("Clean mode - deleting all namespaces and resources");
        netns::cleanup_namespaces(&config)?;
        pidfile::remove_pidfile(&args.pidfile);
        pidfile::remove_state_file(&args.statefile);
        return Ok(());
    }

    if pidfile::check_stale_pidfile(&args.pidfile, &args.statefile)? {
        warn!("Detected stale state from previous abnormal exit, running recovery cleanup");
        // 基本恢复：杀进程 + 删除命名空间
        pidfile::recover_cleanup_basic(&args.pidfile, &args.statefile)?;
        // 完整清理：veth-peer、iptables 规则、DNS 目录（需要 config 信息）
        // 注意：cleanup_namespaces 内部会检查 ns 是否存在，已删的 ns 会跳过
        let _ = netns::cleanup_namespaces(&config);
        // 清理 pidfile/statefile（恢复完成）
        pidfile::remove_pidfile(&args.pidfile);
        pidfile::remove_state_file(&args.statefile);
    }

    // 创建 stop flag 用于信号处理（在任何操作前创建）
    let stop_flag = Arc::new(AtomicBool::new(false));

    // 注册信号处理器（在任何操作前注册，确保能捕获 SIGTERM/SIGINT/SIGHUP）
    register_signal_handlers(stop_flag.clone())?;

    if args.dry_run {
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

        // 写入 PID 文件（供 OpenRC 管理和崩溃恢复）
        pidfile::write_pidfile(&args.pidfile)?;

        // 记录即将创建的命名空间列表（用于崩溃恢复）
        let ns_names: Vec<String> = config.netns_list.iter().map(|n| n.name.clone()).collect();
        pidfile::write_state_file(&args.statefile, &ns_names)?;

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

        // 正常退出，清理 pidfile 和 statefile
        pidfile::remove_pidfile(&args.pidfile);
        pidfile::remove_state_file(&args.statefile);
    }

    info!("netns-r completed successfully");
    Ok(())
}

/// 注册信号处理器
/// 所有信号处理器只做 AtomicBool::store(true)，不做任何 IO/logging（async-signal-safe）
fn register_signal_handlers(_stop_flag: Arc<AtomicBool>) -> Result<()> {
    // SIGINT/SIGTERM → 设置 SUPERVISOR_STOP_FLAG
    extern "C" fn handle_stop(_sig: i32) {
        supervisor::SUPERVISOR_STOP_FLAG.store(true, Ordering::Relaxed);
    }
    let sa_stop = signal::SigAction::new(
        signal::SigHandler::Handler(handle_stop),
        signal::SaFlags::SA_RESTART,
        signal::SigSet::empty(),
    );
    unsafe {
        signal::sigaction(Signal::SIGINT, &sa_stop)?;
        signal::sigaction(Signal::SIGTERM, &sa_stop)?;
    }

    // SIGHUP → 设置 RELOAD_FLAG（OpenRC reload 语义，不触发停止）
    extern "C" fn handle_sighup(_sig: i32) {
        supervisor::set_reload_flag();
    }
    let sa_hup = signal::SigAction::new(
        signal::SigHandler::Handler(handle_sighup),
        signal::SaFlags::SA_RESTART,
        signal::SigSet::empty(),
    );
    unsafe {
        signal::sigaction(Signal::SIGHUP, &sa_hup)?;
    }

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

use crate::config::{get_service_list, Config, ServiceConfig};
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 全局 stop flag（用于信号处理）
pub static SUPERVISOR_STOP_FLAG: AtomicBool = AtomicBool::new(false);

/// 设置 supervisor stop flag
pub fn set_supervisor_stop_flag() {
    SUPERVISOR_STOP_FLAG.store(true, Ordering::Relaxed);
}

/// 服务状态
#[derive(Debug)]
pub enum ServiceState {
    Running {
        child: Child,
        start_time: Instant,
        restart_count: u32,
    },
    Stopped,
    Failed {
        retry_count: u32,
        last_attempt: Instant,
    },
    Restarting {
        next_retry: Instant,
        retry_count: u32,
    },
}

/// 服务跟踪器
pub struct ServiceTracker {
    states: HashMap<String, ServiceState>,
    check_interval: Duration,
}

impl ServiceTracker {
    pub fn new(check_interval: Duration) -> Self {
        Self {
            states: HashMap::new(),
            check_interval,
        }
    }

    /// 注册服务
    pub fn register_service(&mut self, service_name: &str) {
        self.states
            .insert(service_name.to_string(), ServiceState::Stopped);
    }

    /// 更新服务状态
    pub fn update_state(&mut self, service_name: &str, state: ServiceState) {
        self.states.insert(service_name.to_string(), state);
    }

    /// 获取服务状态
    pub fn get_state(&self, service_name: &str) -> Option<&ServiceState> {
        self.states.get(service_name)
    }

    /// 检查是否需要重启
    /// 持续重试，使用指数退避（最大 60 秒）
    pub fn should_restart(&self, service_name: &str) -> bool {
        match self.get_state(service_name) {
            Some(ServiceState::Failed {
                retry_count,
                last_attempt,
                ..
            }) => {
                // 计算退避时间（指数退避，最大 60 秒）
                let backoff_secs = std::cmp::min(60, 2u64.pow(*retry_count));
                let backoff = Duration::from_secs(backoff_secs);
                let elapsed = last_attempt.elapsed();

                elapsed >= backoff
            }
            Some(ServiceState::Restarting { next_retry, .. }) => Instant::now() >= *next_retry,
            _ => false,
        }
    }

    /// 获取所有需要重启的服务
    pub fn get_services_to_restart(&self) -> Vec<String> {
        self.states
            .keys()
            .filter(|name| self.should_restart(name))
            .cloned()
            .collect()
    }
}

/// 进程守护器
pub struct Supervisor {
    tracker: ServiceTracker,
    config: Config,
    stop_flag: Arc<AtomicBool>,
}

impl Supervisor {
    pub fn new(config: Config, stop_flag: Arc<AtomicBool>) -> Self {
        let check_interval = Duration::from_secs(config.global.supervisord.check_interval);
        Self {
            tracker: ServiceTracker::new(check_interval),
            config,
            stop_flag,
        }
    }

    /// 启动监控循环
    pub fn run(&mut self) -> Result<()> {
        info!("Starting supervisor...");

        // 注册所有服务
        self.register_all_services()?;

        // 启动所有服务
        self.start_all_services()?;

        // 监控循环（每秒检查一次 stop_flag，确保快速响应信号）
        let check_interval = self.tracker.check_interval;
        let mut elapsed = Duration::ZERO;
        while !self.stop_flag.load(Ordering::Relaxed)
            && !SUPERVISOR_STOP_FLAG.load(Ordering::Relaxed)
        {
            if elapsed >= check_interval {
                self.check_services()?;
                self.restart_failed_services()?;
                elapsed = Duration::ZERO;
            }

            // 每秒检查一次 stop_flag
            std::thread::sleep(Duration::from_secs(1));
            elapsed += Duration::from_secs(1);
        }

        info!("Supervisor received stop signal, shutting down...");

        // 停止所有运行中的服务进程
        self.stop_all_services();

        Ok(())
    }

    /// 注册所有服务
    fn register_all_services(&mut self) -> Result<()> {
        for netns_config in &self.config.netns_list {
            let services = get_service_list(netns_config, &self.config);
            for service_config in services {
                let service_id = format!("{}/{}", netns_config.name, service_config.name);
                self.tracker.register_service(&service_id);
            }
        }

        Ok(())
    }

    /// 启动所有服务
    fn start_all_services(&mut self) -> Result<()> {
        // 先收集所有需要启动的服务信息
        let mut services_to_start = Vec::new();

        for netns_config in &self.config.netns_list {
            let ns_name = netns_config.name.clone();
            let services = get_service_list(netns_config, &self.config);

            for service_config in services {
                services_to_start.push((ns_name.clone(), service_config.clone()));
            }
        }

        // 然后启动服务
        for (ns_name, service_config) in services_to_start {
            self.start_service_process(&ns_name, &service_config, 0)?;
        }

        Ok(())
    }

    /// 启动服务进程
    /// restart_count: 当前的重启次数（首次启动为 0）
    fn start_service_process(
        &mut self,
        ns_name: &str,
        service_config: &ServiceConfig,
        restart_count: u32,
    ) -> Result<()> {
        let service_id = format!("{}/{}", ns_name, service_config.name);

        info!("Starting service process: {}", service_id);

        // 检查命令非空
        if service_config.command.trim().is_empty() {
            return Err(anyhow!("Empty command for service {}", service_config.name));
        }

        // 准备日志文件
        let log_file = prepare_log_file(service_config, ns_name)?;

        // 替换命令中的变量
        let command = crate::config::replace_variables(
            &service_config.command,
            ns_name,
            &service_config.name,
        );

        // 构建完整命令（包含日志重定向，日志路径做 shell 转义）
        let full_command = if let Some(log_path) = &log_file {
            format!("{} > {} 2>&1", command, shell_quote(log_path))
        } else {
            command
        };

        // 准备环境变量（service 级别 + 标准变量）
        let env_vars = prepare_environment(service_config, ns_name);

        // 在后台启动进程
        // 环境变量通过 Command::env 安全注入，避免 shell 注入风险
        let mut cmd = Command::new("ip");
        cmd.args(["netns", "exec", ns_name, "sh", "-c", &full_command]);
        for (k, v) in &env_vars {
            cmd.env(k, v);
        }
        let child = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;

        let pid = child.id();

        // 更新状态（持有 Child handle，用 try_wait 检查存活，避免 PID 复用问题）
        self.tracker.update_state(
            &service_id,
            ServiceState::Running {
                child,
                start_time: Instant::now(),
                restart_count,
            },
        );

        info!("Service {} started with PID {}", service_id, pid);

        Ok(())
    }

    /// 检查服务状态
    /// 使用 Child::try_wait() 代替 kill -0，避免 PID 复用导致的误判
    fn check_services(&mut self) -> Result<()> {
        debug!("Checking services...");

        // 收集已退出的服务，避免在迭代中修改 HashMap
        let mut to_fail: Vec<(String, u32)> = Vec::new();

        for (service_id, state) in &mut self.tracker.states {
            if let ServiceState::Running {
                child,
                restart_count,
                start_time,
            } = state
            {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        warn!(
                            "Service {} exited with status {:?} (ran for {:?})",
                            service_id,
                            status.code(),
                            start_time.elapsed()
                        );
                        to_fail.push((service_id.clone(), *restart_count));
                    }
                    Ok(None) => {
                        // 进程仍在运行
                    }
                    Err(e) => {
                        warn!("Error checking service {}: {}", service_id, e);
                        to_fail.push((service_id.clone(), *restart_count));
                    }
                }
            }
        }

        for (service_id, retry_count) in to_fail {
            self.tracker.update_state(
                &service_id,
                ServiceState::Failed {
                    retry_count,
                    last_attempt: Instant::now(),
                },
            );
        }

        Ok(())
    }

    /// 重启失败的服务
    fn restart_failed_services(&mut self) -> Result<()> {
        let services_to_restart = self.tracker.get_services_to_restart();

        for service_id in services_to_restart {
            info!("Restarting service: {}", service_id);

            // 解析服务 ID
            let parts: Vec<&str> = service_id.split('/').collect();
            if parts.len() != 2 {
                error!("Invalid service ID: {}", service_id);
                continue;
            }

            let ns_name = parts[0];
            let service_name = parts[1];

            // 找到服务配置（克隆以避免借用问题）
            let service_config = find_service_config(&self.config, ns_name, service_name).cloned();

            if let Some(service_config) = service_config {
                // 获取当前的重试计数
                let current_retry_count = match self.tracker.get_state(&service_id) {
                    Some(ServiceState::Failed { retry_count, .. }) => *retry_count,
                    Some(ServiceState::Restarting { retry_count, .. }) => *retry_count,
                    _ => 0,
                };

                // 更新状态为 Restarting，保留重试计数（退避时间最大 60 秒）
                let backoff_secs = std::cmp::min(60, 2u64.pow(current_retry_count));
                self.tracker.update_state(
                    &service_id,
                    ServiceState::Restarting {
                        next_retry: Instant::now() + Duration::from_secs(backoff_secs),
                        retry_count: current_retry_count,
                    },
                );

                // 尝试重启（直接传入正确的 restart_count，避免事后修改状态）
                match self.start_service_process(ns_name, &service_config, current_retry_count + 1)
                {
                    Ok(_) => {
                        info!(
                            "Service {} restarted successfully (restart_count={})",
                            service_id,
                            current_retry_count + 1
                        );
                    }
                    Err(e) => {
                        error!(
                            "Failed to restart service {}: {} (retry_count={})",
                            service_id,
                            e,
                            current_retry_count + 1
                        );
                        // 更新状态为 Failed，重试计数增加
                        self.tracker.update_state(
                            &service_id,
                            ServiceState::Failed {
                                retry_count: current_retry_count + 1,
                                last_attempt: Instant::now(),
                            },
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// 停止所有运行中的服务进程
    fn stop_all_services(&mut self) {
        for (service_id, state) in &mut self.tracker.states {
            if let ServiceState::Running { child, .. } = state {
                let pid = child.id();
                info!("Stopping service {} (PID {})", service_id, pid);
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

/// 查找服务配置
fn find_service_config<'a>(
    config: &'a Config,
    ns_name: &str,
    service_name: &str,
) -> Option<&'a ServiceConfig> {
    for netns_config in &config.netns_list {
        if netns_config.name == ns_name {
            let services = get_service_list(netns_config, config);
            return services.iter().find(|s| s.name == service_name);
        }
    }
    None
}

/// 对字符串进行 shell 单引号转义，防止日志路径中的特殊字符破坏 shell 命令
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 准备日志文件
fn prepare_log_file(service_config: &ServiceConfig, ns_name: &str) -> Result<Option<String>> {
    let log_config = match &service_config.log_file {
        Some(config) => config,
        None => return Ok(None),
    };

    // 如果 path 为 None（配置为 no 或不配置），则不写日志文件
    let path_template = match &log_config.path {
        Some(path) => path,
        None => return Ok(None),
    };

    let log_path = path_template
        .replace("$(netns)", ns_name)
        .replace("$(service)", &service_config.name);

    if let Some(parent) = std::path::Path::new(&log_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    Ok(Some(log_path))
}

/// 准备环境变量（service 级别 + 标准变量）
fn prepare_environment(service_config: &ServiceConfig, ns_name: &str) -> HashMap<String, String> {
    let mut env = HashMap::new();

    // 添加服务特定的环境变量
    if let Some(env_list) = &service_config.env {
        for env_var in env_list {
            env.insert(env_var.name.clone(), env_var.value.clone());
        }
    }

    // 添加标准环境变量
    env.insert("NETNS_NAME".to_string(), ns_name.to_string());
    env.insert("SERVICE_NAME".to_string(), service_config.name.clone());

    env
}

/// 启动 supervisor
pub fn start_supervisor(config: &Config, stop_flag: Arc<AtomicBool>) -> Result<()> {
    let mut supervisor = Supervisor::new(config.clone(), stop_flag);
    supervisor.run()
}

/// oneshot 模式：启动所有服务但不监控结果
pub fn start_services_oneshot(config: &Config) -> Result<()> {
    info!("Starting services in oneshot mode (fork and forget)...");

    for netns_config in &config.netns_list {
        let ns_name = &netns_config.name;
        let services = get_service_list(netns_config, config);

        for service_config in services {
            let service_id = format!("{}/{}", ns_name, service_config.name);
            info!("Starting service: {}", service_id);

            // 准备日志文件
            let log_file = prepare_log_file(service_config, ns_name)?;

            // 替换命令中的变量
            let command = crate::config::replace_variables(
                &service_config.command,
                ns_name,
                &service_config.name,
            );

            // 构建完整命令（包含日志重定向）
            let full_command = if let Some(log_path) = &log_file {
                format!("{} > {} 2>&1", command, shell_quote(log_path))
            } else {
                command
            };

            // 准备环境变量（service 级别 + 标准变量）
            let env_vars = prepare_environment(service_config, ns_name);

            // 在后台启动进程（fork and forget）
            // 环境变量通过 Command::env 安全注入
            let mut cmd = Command::new("ip");
            cmd.args(["netns", "exec", ns_name, "sh", "-c", &full_command]);
            for (k, v) in &env_vars {
                cmd.env(k, v);
            }

            match cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
                Ok(child) => {
                    info!("Service {} started with PID {}", service_id, child.id());
                    // 不等待子进程，直接继续
                }
                Err(e) => {
                    warn!("Failed to start service {}: {}", service_id, e);
                }
            }
        }
    }

    info!("All services started in oneshot mode");
    Ok(())
}

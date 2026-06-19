use anyhow::{Result, anyhow};
use log::{info, warn, error, debug};
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::config::{Config, ServiceConfig, get_service_list};

/// 全局 stop flag（用于信号处理）
pub static SUPERVISOR_STOP_FLAG: AtomicBool = AtomicBool::new(false);

/// 设置 supervisor stop flag
pub fn set_supervisor_stop_flag() {
    SUPERVISOR_STOP_FLAG.store(true, Ordering::Relaxed);
}

/// 服务状态
#[derive(Debug, Clone)]
pub enum ServiceState {
    Running {
        pid: u32,
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
        retry_count: u32,  // 携带重试计数
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
        self.states.insert(service_name.to_string(), ServiceState::Stopped);
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
            Some(ServiceState::Failed { retry_count, last_attempt, .. }) => {
                // 计算退避时间（指数退避，最大 60 秒）
                let backoff_secs = std::cmp::min(60, 2u64.pow(*retry_count));
                let backoff = Duration::from_secs(backoff_secs);
                let elapsed = last_attempt.elapsed();
                
                elapsed >= backoff
            }
            Some(ServiceState::Restarting { next_retry, .. }) => {
                Instant::now() >= *next_retry
            }
            _ => false,
        }
    }
    
    /// 获取所有需要重启的服务
    pub fn get_services_to_restart(&self) -> Vec<String> {
        self.states.keys()
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
        while !self.stop_flag.load(Ordering::Relaxed) && !SUPERVISOR_STOP_FLAG.load(Ordering::Relaxed) {
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
            self.start_service_process(&ns_name, &service_config)?;
        }
        
        Ok(())
    }
    
    /// 启动服务进程
    fn start_service_process(&mut self, ns_name: &str, service_config: &ServiceConfig) -> Result<()> {
        let service_id = format!("{}/{}", ns_name, service_config.name);
        
        info!("Starting service process: {}", service_id);
        
        // 准备命令
        let command_parts: Vec<&str> = service_config.command.split_whitespace().collect();
        if command_parts.is_empty() {
            return Err(anyhow!("Empty command for service {}", service_config.name));
        }
        
        // 准备日志文件
        let log_file = prepare_log_file(service_config, ns_name)?;
        
        // 替换命令中的变量
        let command = crate::config::replace_variables(&service_config.command, ns_name, &service_config.name);
        
        // 构建完整命令
        let full_command = if let Some(log_path) = &log_file {
            format!("{} > {} 2>&1", command, log_path)
        } else {
            command
        };
        
        // 准备环境变量（service 级别 + 标准变量）
        let env_vars = prepare_environment(service_config, ns_name);
        
        // 构建环境变量字符串
        let env_string = if env_vars.is_empty() {
            String::new()
        } else {
            let env_pairs: Vec<String> = env_vars.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!("{} ", env_pairs.join(" "))
        };
        
        // 构建完整命令（包含环境变量）
        let full_command_with_env = format!("{}{}", env_string, full_command);
        
        // 在后台启动进程
        let child = Command::new("ip")
            .args(["netns", "exec", ns_name, "sh", "-c", &full_command_with_env])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let pid = child.id();
        
        // 更新状态
        self.tracker.update_state(&service_id, ServiceState::Running {
            pid,
            start_time: Instant::now(),
            restart_count: 0,
        });
        
        info!("Service {} started with PID {}", service_id, pid);
        
        // 将子进程放入后台监控
        std::thread::spawn(move || {
            let _ = child.wait_with_output();
        });
        
        Ok(())
    }
    
    /// 检查服务状态
    fn check_services(&mut self) -> Result<()> {
        debug!("Checking services...");
        
        let service_ids: Vec<String> = self.tracker.states.keys().cloned().collect();
        
        for service_id in service_ids {
            if let Some(ServiceState::Running { pid, restart_count, .. }) = self.tracker.get_state(&service_id) {
                let current_restart_count = *restart_count;
                // 检查进程是否还在运行
                if !is_process_running(*pid) {
                    warn!("Service {} (PID {}) is no longer running (restart_count={})", service_id, pid, current_restart_count);
                    self.tracker.update_state(&service_id, ServiceState::Failed {
                        retry_count: current_restart_count,  // 保持之前的重试计数
                        last_attempt: Instant::now(),
                    });
                }
            }
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
                    Some(ServiceState::Restarting { retry_count, .. }) => *retry_count, // 从 Restarting 状态获取重试计数
                    _ => 0,
                };
                
                // 更新状态为 Restarting，保留重试计数（退避时间最大 60 秒）
                let backoff_secs = std::cmp::min(60, 2u64.pow(current_retry_count));
                self.tracker.update_state(&service_id, ServiceState::Restarting {
                    next_retry: Instant::now() + Duration::from_secs(backoff_secs),
                    retry_count: current_retry_count,
                });
                
                // 尝试重启
                match self.start_service_process(ns_name, &service_config) {
                    Ok(_) => {
                        info!("Service {} restarted successfully (restart_count={})", service_id, current_retry_count + 1);
                        // 覆盖状态，设置正确的重启计数
                        if let Some(ServiceState::Running { pid, start_time, .. }) = self.tracker.get_state(&service_id) {
                            self.tracker.update_state(&service_id, ServiceState::Running {
                                pid: *pid,
                                start_time: *start_time,
                                restart_count: current_retry_count + 1,
                            });
                        }
                    }
                    Err(e) => {
                        error!("Failed to restart service {}: {} (retry_count={})", service_id, e, current_retry_count + 1);
                        // 更新状态为 Failed，重试计数增加
                        self.tracker.update_state(&service_id, ServiceState::Failed {
                            retry_count: current_retry_count + 1,
                            last_attempt: Instant::now(),
                        });
                    }
                }
            }
        }
        
        Ok(())
    }
}

/// 检查进程是否在运行
fn is_process_running(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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

/// 准备日志文件
fn prepare_log_file(
    service_config: &ServiceConfig,
    ns_name: &str,
) -> Result<Option<String>> {
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
fn prepare_environment(
    service_config: &ServiceConfig,
    ns_name: &str,
) -> HashMap<String, String> {
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
            let command = crate::config::replace_variables(&service_config.command, ns_name, &service_config.name);
            
            // 构建完整命令
            let full_command = if let Some(log_path) = &log_file {
                format!("{} > {} 2>&1", command, log_path)
            } else {
                command
            };
            
            // 准备环境变量（service 级别 + 标准变量）
            let env_vars = prepare_environment(service_config, ns_name);
            
            // 构建环境变量字符串
            let env_string = if env_vars.is_empty() {
                String::new()
            } else {
                let env_pairs: Vec<String> = env_vars.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                format!("{} ", env_pairs.join(" "))
            };
            
            // 构建完整命令（包含环境变量）
            let full_command_with_env = format!("{}{}", env_string, full_command);
            
            // 在后台启动进程（fork and forget）
            match Command::new("ip")
                .args(["netns", "exec", ns_name, "sh", "-c", &full_command_with_env])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
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
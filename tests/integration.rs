use anyhow::Result;
use std::path::Path;

// 导入主程序模块
use netns_r::config;
use netns_r::env_check;

#[test]
fn test_config_loading() -> Result<()> {
    // 测试加载示例配置文件
    let config_path = "conf/config.example.simple.yaml";
    if Path::new(config_path).exists() {
        let config = config::load_config(config_path)?;

        // 验证配置结构
        assert!(!config.global.log.level.is_empty());
        assert!(!config.global.log.path.is_empty());
        assert!(config.global.supervisord.check_interval > 0);

        // 验证默认配置（log_file.path 可选）
        // assert!(!config.default.log_file.path.is_empty());

        // 验证网络命名空间配置
        assert!(!config.netns_list.is_empty());

        for netns_config in &config.netns_list {
            assert!(!netns_config.name.is_empty());

            // 验证网络配置
            if config.default.use_raw_dev {
                assert!(
                    netns_config.network.raw_dev.is_some(),
                    "raw-dev should be specified when use-raw-dev is true"
                );
            } else {
                assert!(
                    netns_config.network.veth_dev.is_some(),
                    "veth-dev should be specified when use-raw-dev is false"
                );
            }

            // 验证 IP 地址格式
            if let Some(ipv4) = &netns_config.network.ipv4_address {
                assert!(
                    ipv4.contains('/'),
                    "IPv4 address should include subnet mask"
                );
            }

            if let Some(ipv6) = &netns_config.network.ipv6_address {
                assert!(
                    ipv6.contains('/'),
                    "IPv6 address should include subnet mask"
                );
            }
        }

        println!("Configuration loaded and validated successfully");
    } else {
        println!("Skipping config loading test: {} not found", config_path);
    }

    Ok(())
}

#[test]
fn test_config_validation() -> Result<()> {
    // 测试配置验证功能
    let config_path = "conf/config.example.simple.yaml";
    if Path::new(config_path).exists() {
        let config = config::load_config(config_path)?;

        // 测试配置验证
        let result = config::validate_config(&config);
        assert!(result.is_ok(), "Configuration validation should pass");

        println!("Configuration validation test passed");
    } else {
        println!("Skipping config validation test: {} not found", config_path);
    }

    Ok(())
}

#[test]
fn test_variable_replacement() {
    // 测试变量替换功能
    let template = "/tmp/netns-$(netns)-$(service).log";
    let result = config::replace_variables(template, "cmc", "dping-server");

    assert_eq!(result, "/tmp/netns-cmc-dping-server.log");

    // 测试多个变量
    let template2 = "$(netns)/$(service)/$(netns)-$(service).conf";
    let result2 = config::replace_variables(template2, "ctc", "ping");

    assert_eq!(result2, "ctc/ping/ctc-ping.conf");

    println!("Variable replacement tests passed");
}

#[test]
fn test_service_list_retrieval() -> Result<()> {
    // 测试获取服务列表功能
    let config_path = "conf/config.example.simple.yaml";
    if Path::new(config_path).exists() {
        let config = config::load_config(config_path)?;

        for netns_config in &config.netns_list {
            let services = config::get_service_list(netns_config, &config);

            // 验证服务列表不为空（除非显式配置为空）
            if let Some(ns_services) = &netns_config.service_list {
                if ns_services.is_empty() {
                    // 命名空间显式配置了空服务列表
                    assert!(
                        services.is_empty(),
                        "Service list should be empty when explicitly set to []"
                    );
                } else {
                    // 命名空间配置了自己的服务列表
                    assert_eq!(services.len(), ns_services.len());
                }
            } else {
                // 使用默认服务列表
                assert_eq!(services.len(), config.default.service_list.len());
            }

            // 验证服务配置
            for service in services {
                assert!(!service.name.is_empty());
                assert!(!service.command.is_empty());
            }
        }

        println!("Service list retrieval tests passed");
    } else {
        println!("Skipping service list test: {} not found", config_path);
    }

    Ok(())
}

#[test]
fn test_environment_check() -> Result<()> {
    // 测试环境检查功能（仅在 Linux 环境中运行）
    if cfg!(target_os = "linux") {
        // 在 Linux 环境中运行环境检查
        let result = env_check::check_environment();

        // 注意：这个测试在容器中可能会失败，因为需要 root 权限
        // 在 CI 环境中可能需要跳过
        match result {
            Ok((system_info, network_devices)) => {
                println!("System info: {:?}", system_info);
                println!("Network devices: {:?}", network_devices);

                // 验证系统信息
                assert!(!system_info.os.is_empty());
                assert!(!system_info.kernel.is_empty());
                assert!(system_info.cpu_cores > 0);
                assert!(system_info.memory_mb > 0);
                assert!(!system_info.virtualization.is_empty());
            }
            Err(e) => {
                println!("Environment check failed (may need root privileges): {}", e);
            }
        }
    } else {
        println!("Skipping environment check test: not running on Linux");
    }

    Ok(())
}

#[test]
fn test_command_executor() -> Result<()> {
    // 测试命令执行器
    use netns_r::command::{CommandExecutor, SystemCommandExecutor};

    let executor = SystemCommandExecutor::new(false);

    // 测试简单命令
    let output = executor.execute("echo", &["hello"])?;
    assert_eq!(output.status, netns_r::command::ExecStatus::Success);
    assert_eq!(output.stdout.trim(), "hello");

    // 测试失败命令
    let output = executor.execute("false", &[])?;
    assert_eq!(output.status, netns_r::command::ExecStatus::Failed);

    // 测试 dry-run 模式
    let executor_dry_run = SystemCommandExecutor::new(true);
    let output = executor_dry_run.execute("echo", &["dry-run"])?;
    assert_eq!(output.status, netns_r::command::ExecStatus::Success);
    assert!(output.stdout.is_empty()); // dry-run 模式不执行命令

    println!("Command executor tests passed");

    Ok(())
}

#[test]
fn test_log_system() -> Result<()> {
    // 测试日志系统
    use netns_r::log_system::LogSystem;

    let _log_system = LogSystem::new("debug", Some("/tmp/test-netns-r.log"))?;

    // 检查日志文件是否创建（LogSystem::new 会创建文件）
    assert!(Path::new("/tmp/test-netns-r.log").exists());

    // 清理测试日志文件
    std::fs::remove_file("/tmp/test-netns-r.log")?;

    println!("Log system tests passed");

    Ok(())
}

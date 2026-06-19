use anyhow::Result;
use chrono::Local;
use colored::Colorize;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

/// 日志系统
pub struct LogSystem {
    level: LevelFilter,
    file: Option<Mutex<std::fs::File>>,
}

impl LogSystem {
    pub fn new(level: &str, file_path: Option<&str>) -> Result<Self> {
        let level = match level.to_lowercase().as_str() {
            "debug" => LevelFilter::Debug,
            "info" => LevelFilter::Info,
            "error" => LevelFilter::Error,
            _ => LevelFilter::Info,
        };

        let file = if let Some(path) = file_path {
            let file = OpenOptions::new().create(true).append(true).open(path)?;
            Some(Mutex::new(file))
        } else {
            None
        };

        Ok(Self { level, file })
    }

    /// 格式化日志消息
    fn format_message(&self, record: &Record) -> String {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let level = match record.level() {
            Level::Error => "ERROR".red().bold(),
            Level::Warn => "WARN".yellow().bold(),
            Level::Info => "INFO".green().bold(),
            Level::Debug => "DEBUG".blue().bold(),
            Level::Trace => "TRACE".purple().bold(),
        };

        format!("{} [{}] {}", timestamp, level, record.args())
    }
}

impl Log for LogSystem {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let message = self.format_message(record);

        // 输出到终端
        println!("{}", message);

        // 输出到文件（如果配置了）
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                // 文件中不使用颜色代码
                let plain_message = format!(
                    "{} [{}] {}",
                    Local::now().format("%Y-%m-%d %H:%M:%S"),
                    record.level(),
                    record.args()
                );
                let _ = writeln!(file, "{}", plain_message);
            }
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.flush();
            }
        }
    }
}

/// 初始化日志系统
/// log_path 来自配置文件 global.log.path
pub fn init(level: &str, log_path: &str) -> Result<()> {
    let log_system = LogSystem::new(level, Some(log_path))?;

    log::set_boxed_logger(Box::new(log_system))?;
    log::set_max_level(match level.to_lowercase().as_str() {
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "error" => LevelFilter::Error,
        _ => LevelFilter::Info,
    });

    Ok(())
}

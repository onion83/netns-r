use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};

/// 全局配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GlobalConfig {
    pub supervisord: SupervisordConfig,
    pub log: LogConfig,
}

/// 自定义布尔值反序列化函数，支持 "yes"/"no" 和 true/false
fn deserialize_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match value {
        serde_yaml::Value::Bool(b) => Ok(b),
        serde_yaml::Value::String(s) => match s.to_lowercase().as_str() {
            "yes" | "true" | "1" => Ok(true),
            "no" | "false" | "0" => Ok(false),
            _ => Err(serde::de::Error::custom(format!(
                "invalid boolean string: {}",
                s
            ))),
        },
        _ => Err(serde::de::Error::custom("invalid boolean value")),
    }
}

/// Supervisord 配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SupervisordConfig {
    #[serde(deserialize_with = "deserialize_bool")]
    pub enable: bool,
    #[serde(rename = "check-interval")]
    pub check_interval: u64,
}

/// 日志配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogConfig {
    pub level: String,
    pub path: String,
}

/// 默认配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DefaultConfig {
    #[serde(rename = "use-raw-dev", deserialize_with = "deserialize_bool")]
    pub use_raw_dev: bool,
    #[serde(rename = "outbound-dev")]
    pub outbound_dev: Option<String>,
    pub network: NetworkConfig,
    #[serde(
        rename = "log-file",
        deserialize_with = "deserialize_optional_log_file",
        default
    )]
    pub log_file: Option<LogFileConfig>,
    #[serde(rename = "service-list")]
    pub service_list: Vec<ServiceConfig>,
}

/// 网络配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NetworkConfig {
    #[serde(rename = "ipv4-gateway")]
    pub ipv4_gateway: Option<String>,
    #[serde(rename = "ipv6-gateway")]
    pub ipv6_gateway: Option<String>,
    #[serde(rename = "dns-servers")]
    pub dns_servers: Option<Vec<String>>,
}

/// 日志文件配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogFileConfig {
    pub path: Option<String>,
}

/// 自定义反序列化函数，支持 log-file: no 或 log-file: {path: "..."} 或 log-file: null
fn deserialize_optional_log_file<'de, D>(deserializer: D) -> Result<Option<LogFileConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match value {
        // log-file: null 或不配置
        serde_yaml::Value::Null => Ok(None),
        // log-file: no 或 log-file: "no"
        serde_yaml::Value::Bool(false) => Ok(None),
        serde_yaml::Value::String(s) if s.to_lowercase() == "no" || s.to_lowercase() == "false" => {
            Ok(None)
        }
        // log-file: {path: "..."}
        serde_yaml::Value::Mapping(_) => {
            let config: LogFileConfig =
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?;
            Ok(Some(config))
        }
        _ => Err(serde::de::Error::custom("invalid log-file value")),
    }
}

/// 服务配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub command: String,
    #[serde(
        rename = "log-file",
        deserialize_with = "deserialize_optional_log_file",
        default
    )]
    pub log_file: Option<LogFileConfig>,
    pub env: Option<Vec<EnvVar>>,
}

/// 环境变量
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// 网络命名空间配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NetnsConfig {
    pub name: String,
    pub network: NetnsNetworkConfig,
    pub env: Option<Vec<EnvVar>>,
    #[serde(rename = "service-list")]
    pub service_list: Option<Vec<ServiceConfig>>,
}

/// 网络命名空间网络配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NetnsNetworkConfig {
    #[serde(rename = "raw-dev")]
    pub raw_dev: Option<String>,
    #[serde(rename = "veth-dev")]
    pub veth_dev: Option<String>,
    #[serde(rename = "ipv4-address")]
    pub ipv4_address: Option<String>,
    #[serde(rename = "ipv4-gateway")]
    pub ipv4_gateway: Option<String>,
    #[serde(rename = "ipv6-address")]
    pub ipv6_address: Option<String>,
    #[serde(rename = "ipv6-gateway")]
    pub ipv6_gateway: Option<String>,
    #[serde(rename = "dns-servers")]
    pub dns_servers: Option<Vec<String>>,
    #[serde(rename = "outbound-dev")]
    pub outbound_dev: Option<String>,
}

/// 主配置结构
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub global: GlobalConfig,
    pub default: DefaultConfig,
    #[serde(rename = "netns-list")]
    pub netns_list: Vec<NetnsConfig>,
}

/// 加载配置文件
pub fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// 变量替换函数
pub fn replace_variables(template: &str, netns_name: &str, service_name: &str) -> String {
    template
        .replace("$(netns)", netns_name)
        .replace("$(service)", service_name)
}

/// 获取服务列表（考虑命名空间特定配置或默认配置）
pub fn get_service_list<'a>(
    netns_config: &'a NetnsConfig,
    global_config: &'a Config,
) -> &'a [ServiceConfig] {
    if let Some(services) = &netns_config.service_list {
        return services;
    }
    &global_config.default.service_list
}

/// 验证配置
pub fn validate_config(config: &Config) -> Result<()> {
    // 检查全局配置
    if config.global.supervisord.check_interval == 0 {
        return Err(anyhow::anyhow!(
            "supervisord check-interval must be greater than 0"
        ));
    }

    // 检查网络命名空间配置
    for netns_config in &config.netns_list {
        if netns_config.name.is_empty() {
            return Err(anyhow::anyhow!("netns name cannot be empty"));
        }

        // 检查网络配置
        let network = &netns_config.network;
        if config.default.use_raw_dev {
            if network.raw_dev.is_none() {
                return Err(anyhow::anyhow!(
                    "raw-dev must be specified when use-raw-dev is true"
                ));
            }
        } else {
            if network.veth_dev.is_none() {
                return Err(anyhow::anyhow!(
                    "veth-dev must be specified when use-raw-dev is false"
                ));
            }
        }

        // 检查 IP 地址格式
        if let Some(ipv4) = &network.ipv4_address {
            if !ipv4.contains('/') {
                return Err(anyhow::anyhow!(
                    "IPv4 address must include subnet mask (e.g., 192.168.1.1/24)"
                ));
            }
        }

        if let Some(ipv6) = &network.ipv6_address {
            if !ipv6.contains('/') {
                return Err(anyhow::anyhow!(
                    "IPv6 address must include subnet mask (e.g., fd83:99::241/64)"
                ));
            }
        }
    }

    Ok(())
}

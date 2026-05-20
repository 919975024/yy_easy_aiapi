//! 配置管理：从 config.toml 加载，支持默认值

use serde::Deserialize;
use std::path::PathBuf;
use tracing::info;

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub admin: AdminConfig,

    #[serde(default)]
    pub concurrency: ConcurrencyConfig,

    #[serde(default)]
    pub pricing: PricingConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    /// AI Mock 模式：不请求上游，直接模拟返回（prompt/cached/output 均为 10 tokens）
    #[serde(default)]
    pub aimock: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_url")]
    pub url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AdminConfig {
    #[serde(default)]
    pub api_token: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConcurrencyConfig {
    #[serde(default = "default_base_points")]
    pub base_points_per_concurrent: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PricingConfig {
    #[serde(default = "default_decimal_places")]
    pub decimal_places: u32,
    #[serde(default = "default_round_up")]
    pub round_up: bool,
    #[serde(default = "default_price_multiplier", rename = "multiplier")]
    pub price_multiplier: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LoggingConfig {
    /// 是否启用 debug 模式（打印外部 HTTP 请求/响应的详细日志）
    #[serde(default)]
    pub debug: bool,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_db_url() -> String {
    "file://db/data.db".to_string()
}
fn default_base_points() -> i64 {
    500
}
fn default_decimal_places() -> u32 {
    4
}
fn default_round_up() -> bool {
    true
}
fn default_price_multiplier() -> String {
    "1.0".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: default_host(),
                port: default_port(),
            },
            database: DatabaseConfig {
                url: default_db_url(),
            },
            admin: AdminConfig {
                api_token: String::new(),
                username: String::new(),
                password: String::new(),
            },
            concurrency: ConcurrencyConfig {
                base_points_per_concurrent: default_base_points(),
            },
            pricing: PricingConfig {
                decimal_places: default_decimal_places(),
                round_up: default_round_up(),
                price_multiplier: default_price_multiplier(),
            },
            logging: LoggingConfig::default(),
            aimock: false,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_db_url(),
        }
    }
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            api_token: String::new(),
            username: String::new(),
            password: String::new(),
        }
    }
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            base_points_per_concurrent: default_base_points(),
        }
    }
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            decimal_places: default_decimal_places(),
            round_up: default_round_up(),
            price_multiplier: default_price_multiplier(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { debug: false }
    }
}

impl AppConfig {
    /// 从配置文件加载，找不到则使用默认值
    pub fn from_file() -> Self {
        let config_path = find_config_path();

        match &config_path {
            Some(path) => {
                info!("加载配置文件: {}", path.display());
                match std::fs::read_to_string(path) {
                    Ok(content) => match toml::from_str(&content) {
                        Ok(config) => {
                            info!("配置加载完成");
                            config
                        }
                        Err(e) => {
                            info!("配置文件解析失败 ({}), 使用默认配置", e);
                            Self::default()
                        }
                    },
                    Err(_) => {
                        info!("读取配置文件失败, 使用默认配置");
                        Self::default()
                    }
                }
            }
            None => {
                info!("未找到 config.toml, 使用默认配置");
                Self::default()
            }
        }
    }

    /// 是否有管理端鉴权
    pub fn admin_auth_enabled(&self) -> bool {
        !self.admin.api_token.is_empty()
    }
}

fn find_config_path() -> Option<PathBuf> {
    if let Ok(exe_path) = std::env::current_exe() {
        let mut dir = exe_path.parent().map(|p| p.to_path_buf());
        for _ in 0..5 {
            if let Some(ref d) = dir {
                let config = d.join("config.toml");
                if config.exists() {
                    return Some(config);
                }
            }
            dir = dir.and_then(|d| d.parent().map(|p| p.to_path_buf()));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd);
        for _ in 0..5 {
            if let Some(ref d) = dir {
                let config = d.join("config.toml");
                if config.exists() {
                    return Some(config);
                }
            }
            dir = dir.and_then(|d| d.parent().map(|p| p.to_path_buf()));
        }
    }

    None
}

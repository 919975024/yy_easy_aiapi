//! 请求 DTO 定义

use serde::Deserialize;

/// 创建渠道请求
#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub api_standard: Option<String>,
    pub price_input: Option<String>,
    pub price_cached: Option<String>,
    pub price_output: Option<String>,
    pub rate_input: Option<String>,
    pub rate_cached: Option<String>,
    pub rate_output: Option<String>,
    pub upstream_url: String,
    pub api_key: Option<String>,
    #[serde(default = "default_channel_type")]
    pub channel_type: String,
    #[serde(default)]
    pub price_reference_url: String,
    /// 上游实际调用的模型名（为空则使用渠道名）
    #[serde(default)]
    pub model: String,
    /// 余额查询接口地址（为空则无法查询余额）
    #[serde(default)]
    pub balance_url: String,
}

/// 更新渠道请求
#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    pub api_standard: Option<String>,
    pub price_input: Option<String>,
    pub price_cached: Option<String>,
    pub price_output: Option<String>,
    pub rate_input: Option<String>,
    pub rate_cached: Option<String>,
    pub rate_output: Option<String>,
    pub upstream_url: Option<String>,
    pub api_key: Option<String>,
    pub channel_type: Option<String>,
    pub price_reference_url: Option<String>,
    pub model: Option<String>,
    pub balance_url: Option<String>,
}

fn default_channel_type() -> String {
    "custom".to_string()
}

/// 生成 Token 请求
#[derive(Debug, Deserialize)]
pub struct GenerateTokenRequest {
    pub label: String,
}

/// 充值请求
#[derive(Debug, Deserialize)]
pub struct RechargeRequest {
    pub token: String,
    pub amount: String,
    #[serde(default)]
    pub remark: String,
}

/// 扣减请求
#[derive(Debug, Deserialize)]
pub struct DeductRequest {
    pub token: String,
    pub amount: String,
    #[serde(default)]
    pub remark: String,
}

/// 流水查询参数
#[derive(Debug, Deserialize)]
pub struct TransactionQuery {
    pub token: Option<String>,
    pub label: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

fn default_page() -> i64 {
    1
}
fn default_page_size() -> i64 {
    20
}

/// Token 列表查询参数
#[derive(Debug, Deserialize)]
pub struct TokenListQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

/// 余额查询参数
#[derive(Debug, Deserialize)]
pub struct BalanceQuery {
    pub token: Option<String>,
    pub label: Option<String>,
}

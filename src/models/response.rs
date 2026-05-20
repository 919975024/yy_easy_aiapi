//! 响应 DTO 定义

use serde::Serialize;

/// 统一 API 响应
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: u16,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self { success: true, data: Some(data), error: None, message: None }
    }
    pub fn success_with_message(data: T, msg: &str) -> Self {
        Self { success: true, data: Some(data), error: None, message: Some(msg.to_string()) }
    }
    pub fn error(code: u16, message: &str) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ErrorDetail { code, message: message.to_string(), detail: None }),
            message: Some(message.to_string()),
        }
    }
}

/// 渠道信息
#[derive(Debug, Clone, Serialize)]
pub struct ChannelInfo {
    pub id: i64,
    pub name: String,
    pub api_standard: String,
    pub price_input: String,
    pub price_cached: String,
    pub price_output: String,
    pub rate_input: String,
    pub rate_cached: String,
    pub rate_output: String,
    pub upstream_url: String,
    pub api_key: String,
    pub channel_type: String,
    pub price_reference_url: String,
    pub model: String,
    pub balance_url: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Token 信息
#[derive(Debug, Clone, Serialize)]
pub struct TokenInfo {
    pub id: i64,
    pub token: String,
    pub label: String,
    pub balance: String,
    pub created_at: String,
    pub max_concurrent: i64,
}

/// 生成 Token 响应
#[derive(Debug, Serialize)]
pub struct TokenGenerated {
    pub token: String,
    pub label: String,
    pub balance: String,
}

/// 余额信息
#[derive(Debug, Serialize)]
pub struct BalanceInfo {
    pub token: String,
    pub label: String,
    pub balance: String,
    pub max_concurrent: i64,
}

/// 流水记录
#[derive(Debug, Serialize)]
pub struct TransactionInfo {
    pub id: i64,
    pub token: String,
    pub token_prefix: String,
    pub label: String,
    pub change_type: String,
    pub amount: String,
    pub balance_after: String,
    pub channel_name: String,
    pub request_id: String,
    pub operator: String,
    pub metadata: String,
    pub remark: String,
    pub created_at: String,
}

/// 分页流水
#[derive(Debug, Serialize)]
pub struct TransactionPage {
    pub items: Vec<TransactionInfo>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// 管理员流水记录（含计费详情）
#[derive(Debug, Serialize)]
pub struct AdminTransactionInfo {
    pub id: i64,
    pub token: String,
    pub token_prefix: String,
    pub label: String,
    pub change_type: String,
    pub amount: String,
    pub balance_after: String,
    pub channel_name: String,
    pub request_id: String,
    pub operator: String,
    pub metadata: String,
    pub remark: String,
    pub created_at: String,
    pub input_tokens: i64,
    pub input_price: String,
    pub cached_tokens: i64,
    pub cached_price: String,
    pub output_tokens: i64,
    pub output_price: String,
    pub multiplier: String,
}

/// 管理员分页流水
#[derive(Debug, Serialize)]
pub struct AdminTransactionPage {
    pub items: Vec<AdminTransactionInfo>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// Token 分页列表
#[derive(Debug, Serialize)]
pub struct TokenPage {
    pub items: Vec<TokenInfo>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// 操作结果
#[derive(Debug, Serialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
}

/// 渠道连通测试响应
#[derive(Debug, Serialize)]
pub struct ChannelTestResponse {
    pub channel_id: i64,
    pub channel_name: String,
    pub ok: bool,
    /// 耗时（毫秒）
    pub elapsed_ms: u128,
    /// 上游 HTTP 状态码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
    /// 错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

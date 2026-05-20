//! 数据模型：使用 stoolap 的 FromRow trait 映射查询结果到 Rust 结构体
//!
//! 金额字段存储为 TEXT 以保持精度，应用层通过 rust_decimal 计算。

use stoolap::api::{FromRow, ResultRow};
use stoolap::Result as StoolapResult;

// ── channels ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Channel {
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

impl FromRow for Channel {
    fn from_row(row: &ResultRow) -> StoolapResult<Self> {
        Ok(Channel {
            id: row.get_by_name("id")?,
            name: row.get_by_name("name")?,
            api_standard: row.get_by_name("api_standard")?,
            price_input: row.get_by_name("price_input")?,
            price_cached: row.get_by_name("price_cached")?,
            price_output: row.get_by_name("price_output")?,
            rate_input: row.get_by_name("rate_input")?,
            rate_cached: row.get_by_name("rate_cached")?,
            rate_output: row.get_by_name("rate_output")?,
            upstream_url: row.get_by_name("upstream_url")?,
            api_key: row.get_by_name("api_key")?,
            channel_type: row.get_by_name("channel_type")?,
            price_reference_url: row.get_by_name("price_reference_url")?,
            model: row.get_by_name("model")?,
            balance_url: row.get_by_name("balance_url")?,
            created_at: row.get_by_name("created_at")?,
            updated_at: row.get_by_name("updated_at")?,
        })
    }
}

// ── tokens ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Token {
    pub id: i64,
    pub token: String,
    pub label: String,
    pub balance: String,
    pub created_at: String,
}

impl FromRow for Token {
    fn from_row(row: &ResultRow) -> StoolapResult<Self> {
        Ok(Token {
            id: row.get_by_name("id")?,
            token: row.get_by_name("token")?,
            label: row.get_by_name("label")?,
            balance: row.get_by_name("balance")?,
            created_at: row.get_by_name("created_at")?,
        })
    }
}

// ── transactions ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Transaction {
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

impl FromRow for Transaction {
    fn from_row(row: &ResultRow) -> StoolapResult<Self> {
        Ok(Transaction {
            id: row.get_by_name("id")?,
            token: row.get_by_name("token")?,
            token_prefix: row.get_by_name("token_prefix")?,
            label: row.get_by_name("label")?,
            change_type: row.get_by_name("change_type")?,
            amount: row.get_by_name("amount")?,
            balance_after: row.get_by_name("balance_after")?,
            channel_name: row.get_by_name("channel_name")?,
            request_id: row.get_by_name("request_id")?,
            operator: row.get_by_name("operator")?,
            metadata: row.get_by_name("metadata")?,
            remark: row.get_by_name("remark")?,
            created_at: row.get_by_name("created_at")?,
            input_tokens: row.get_by_name("input_tokens")?,
            input_price: row.get_by_name("input_price")?,
            cached_tokens: row.get_by_name("cached_tokens")?,
            cached_price: row.get_by_name("cached_price")?,
            output_tokens: row.get_by_name("output_tokens")?,
            output_price: row.get_by_name("output_price")?,
            multiplier: row.get_by_name("multiplier")?,
        })
    }
}

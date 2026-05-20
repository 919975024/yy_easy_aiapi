//! 统一错误类型
//!
//! 每个错误构造时自动捕获调用栈，IntoResponse 时打印完整链。

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::backtrace::Backtrace;
use std::error::Error;
use tracing::error;

#[derive(Debug)]
pub enum AppError {
    Database(stoolap::Error, Backtrace),
    HttpRequest(reqwest::Error, Backtrace),
    Serialization(serde_json::Error, Backtrace),
    Business(StatusCode, String, Backtrace),
    Unauthorized(String, Backtrace),
    NotFound(String, Backtrace),
    ConfigError(String, Backtrace),
    InsufficientBalance(String, Backtrace),
    RateLimited(String, Backtrace),
}

macro_rules! bt {
    () => { Backtrace::capture() };
}

impl AppError {
    pub fn business(status: StatusCode, msg: impl Into<String>) -> Self {
        AppError::Business(status, msg.into(), bt!())
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        AppError::Unauthorized(msg.into(), bt!())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        AppError::NotFound(msg.into(), bt!())
    }
    pub fn config_error(msg: impl Into<String>) -> Self {
        AppError::ConfigError(msg.into(), bt!())
    }
    pub fn insufficient_balance(msg: impl Into<String>) -> Self {
        AppError::InsufficientBalance(msg.into(), bt!())
    }
    pub fn rate_limited(msg: impl Into<String>) -> Self {
        AppError::RateLimited(msg.into(), bt!())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Database(e, _) => write!(f, "数据库错误: {}", e),
            AppError::HttpRequest(e, _) => write!(f, "HTTP请求错误: {}", e),
            AppError::Serialization(e, _) => write!(f, "序列化错误: {}", e),
            AppError::Business(_, msg, _) => write!(f, "{}", msg),
            AppError::Unauthorized(msg, _) => write!(f, "未授权: {}", msg),
            AppError::NotFound(msg, _) => write!(f, "未找到: {}", msg),
            AppError::ConfigError(msg, _) => write!(f, "配置错误: {}", msg),
            AppError::InsufficientBalance(msg, _) => write!(f, "余额不足: {}", msg),
            AppError::RateLimited(msg, _) => write!(f, "限流: {}", msg),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Database(e, _) => Some(e),
            AppError::HttpRequest(e, _) => Some(e),
            AppError::Serialization(e, _) => Some(e),
            _ => None,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let backtrace = match &self {
            AppError::Database(_, bt) => bt,
            AppError::HttpRequest(_, bt) => bt,
            AppError::Serialization(_, bt) => bt,
            AppError::Business(_, _, bt) => bt,
            AppError::Unauthorized(_, bt) => bt,
            AppError::NotFound(_, bt) => bt,
            AppError::ConfigError(_, bt) => bt,
            AppError::InsufficientBalance(_, bt) => bt,
            AppError::RateLimited(_, bt) => bt,
        };

        let (status, message) = match &self {
            AppError::Database(..) => (StatusCode::INTERNAL_SERVER_ERROR, "数据库操作失败"),
            AppError::HttpRequest(..) => (StatusCode::BAD_GATEWAY, "上游服务调用失败"),
            AppError::Serialization(..) => (StatusCode::INTERNAL_SERVER_ERROR, "数据处理失败"),
            AppError::Business(code, msg, _) => (*code, msg.as_str()),
            AppError::Unauthorized(msg, _) => (StatusCode::UNAUTHORIZED, msg.as_str()),
            AppError::NotFound(msg, _) => (StatusCode::NOT_FOUND, msg.as_str()),
            AppError::ConfigError(..) => (StatusCode::INTERNAL_SERVER_ERROR, "配置错误"),
            AppError::InsufficientBalance(msg, _) => (StatusCode::FORBIDDEN, msg.as_str()),
            AppError::RateLimited(msg, _) => (StatusCode::TOO_MANY_REQUESTS, msg.as_str()),
        };

        // 打印完整错误链 + 调用栈
        let mut chain = format!("{}", self);
        let mut src = self.source();
        while let Some(e) = src {
            chain.push_str(&format!("\n  caused by: {}", e));
            src = e.source();
        }
        error!("发生异常: {} → HTTP {}\n{}", chain, status.as_u16(), backtrace);

        let body = json!({
            "success": false,
            "error": {
                "code": status.as_u16(),
                "message": message,
                "detail": chain
            }
        });

        (status, Json(body)).into_response()
    }
}

impl From<stoolap::Error> for AppError {
    fn from(e: stoolap::Error) -> Self {
        AppError::Database(e, bt!())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::HttpRequest(e, bt!())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Serialization(e, bt!())
    }
}

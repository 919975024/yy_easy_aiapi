//! yy_easy_aiapi — 轻量级 AI 代理网关
//!
//! 模块结构：
//! - config: 配置加载
//! - error: 统一错误类型
//! - db: 数据层（stoolap Database + FromRow 模型）
//! - models: 请求/响应 DTO
//! - services: 业务逻辑
//! - handlers: HTTP 处理器
//! - middleware: 中间件（鉴权、日志）
//! - state: 全局应用状态

pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod openapi;
pub mod services;
pub mod snowflake;
pub mod state;

#[cfg(test)]
mod tests;

use axum::{
    middleware as axum_middleware,
    routing::{delete, get, post, put},
    Router,
};
use tracing::info;
use tracing_subscriber::prelude::*;

use crate::config::AppConfig;
use crate::db::init_database;
use crate::middleware::auth::{AdminSessionLayer, ApiAuthLayer};
use crate::middleware::logging::logging_middleware;
use crate::state::AppState;

/// 初始化日志系统（stdout + 日滚动文件）
///
/// `debug`: 若为 true，日志级别降为 DEBUG，输出外部 HTTP 请求/响应全量日志。
pub fn init_logging(debug: bool) {
    let log_dir = "logs";
    std::fs::create_dir_all(log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(log_dir, "app.log");

    let level = if debug {
        tracing_subscriber::filter::LevelFilter::DEBUG
    } else {
        tracing_subscriber::filter::LevelFilter::INFO
    };

    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .with_file(true)
        .with_ansi(true)
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339());

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_appender)
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .with_file(true)
        .with_ansi(false)
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339());

    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .with(level)
        .init();
}

/// 构建路由
pub fn build_router(state: AppState) -> Router {
    let prefix = "/yy_easy_aiapi";

    // ── AI 代理（使用自身 Token 鉴权，非 api_token）──
    let ai_routes = Router::new()
        .route(&format!("{prefix}/v1/chat/completions"), post(handlers::ask::chat_completions));

    // ── 非管理 API（需要 api_token 鉴权）──
    let api_routes = Router::new()
        .route(&format!("{prefix}/api/channels"), get(handlers::channel::list_channels))
        .route(&format!("{prefix}/api/channels"), post(handlers::channel::create_channel))
        .route(&format!("{prefix}/api/channels/:id"), put(handlers::channel::update_channel))
        .route(&format!("{prefix}/api/channels/:id"), delete(handlers::channel::delete_channel))
        .route(&format!("{prefix}/api/channels/:id/test"), post(handlers::channel::test_channel_connection))
        .route(&format!("{prefix}/api/token/generate"), post(handlers::token::generate_token))
        .route(&format!("{prefix}/api/tokens"), get(handlers::token::list_tokens))
        .route(&format!("{prefix}/api/tokens/:token"), delete(handlers::token::delete_token))
        .route(&format!("{prefix}/api/token/recharge"), post(handlers::balance::recharge))
        .route(&format!("{prefix}/api/token/deduct"), post(handlers::balance::deduct))
        .route(&format!("{prefix}/api/token/balance"), get(handlers::balance::get_balance))
        .route(&format!("{prefix}/api/token/transactions"), get(handlers::transaction::list_transactions))
        .layer(ApiAuthLayer::new(state.clone()))
        .with_state(state.clone());

    // ── 管理后台页面（需要 admin session 鉴权）──
    let admin_pages = Router::new()
        .route(&format!("{prefix}/admin"), get(handlers::web::channels_page))
        .route(&format!("{prefix}/admin/tokens"), get(handlers::web::tokens_page))
        .route(&format!("{prefix}/admin/transactions"), get(handlers::web::transactions_page))
        .route(&format!("{prefix}/admin/recharge"), get(handlers::web::recharge_page))
        .route(&format!("{prefix}/admin/pricing"), get(handlers::web::pricing_page))
        .layer(AdminSessionLayer::new(state.clone()))
        .with_state(state.clone());

    // ── 管理 API（需要 api_token 鉴权）──
    let admin_apis = Router::new()
        .route(&format!("{prefix}/admin/pricing/api"), get(handlers::pricing::get_pricing))
        .route(&format!("{prefix}/admin/pricing/api"), post(handlers::pricing::update_pricing))
        .route(&format!("{prefix}/admin/transactions/api"), get(handlers::transaction::admin_list_transactions))
        .layer(ApiAuthLayer::new(state.clone()))
        .with_state(state.clone());

    // ── Admin 登录/登出（无需鉴权）──
    let admin_auth_routes = Router::new()
        .route(&format!("{prefix}/admin/login"), get(handlers::admin_auth::login_page))
        .route(&format!("{prefix}/admin/login/api"), post(handlers::admin_auth::login_api))
        .route(&format!("{prefix}/admin/logout/api"), post(handlers::admin_auth::logout_api));

    // ── Swagger UI（手动实现，避免 utoipa 运行时解析问题）──
    let swagger_json_route = Router::new()
        .route(&format!("{prefix}/api-doc/openapi.json"), get(openapi::openapi_spec));

    let swagger_ui_route = Router::new()
        .route(&format!("{prefix}/swagger"), get(openapi::swagger_ui));

    Router::new()
        .merge(ai_routes)
        .merge(api_routes)
        .merge(admin_pages)
        .merge(admin_apis)
        .merge(admin_auth_routes)
        .merge(swagger_json_route)
        .merge(swagger_ui_route)
        .layer(axum_middleware::from_fn(logging_middleware))
        .with_state(state)
}

/// 构建应用状态
pub fn build_app_state(config: AppConfig) -> AppState {
    let db = init_database(&config.database.url)
        .expect("数据库初始化失败");

    info!(
        "数据库已就绪: {}, port={}",
        config.database.url, config.server.port
    );

    AppState::new(db, config)
}
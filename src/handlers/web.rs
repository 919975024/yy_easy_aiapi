//! Web 控制台页面处理器（模板已编译时嵌入二进制）

use std::collections::HashMap;
use std::sync::OnceLock;
use axum::{extract::State, response::Html};
use tera::{Context, Tera};

use crate::error::AppError;
use crate::state::AppState;

static TERA: OnceLock<Tera> = OnceLock::new();

fn get_tera() -> &'static Tera {
    TERA.get_or_init(|| crate::templates::create_tera().expect("模板加载失败"))
}

/// 渠道管理页面 GET /admin
pub async fn channels_page(
    State(state): State<AppState>,
) -> Result<Html<String>, AppError> {
    let channels = state.channel_service.list()?;
    let mut ctx = Context::new();
    ctx.insert("title", "渠道管理");
    ctx.insert("channels", &channels);
    ctx.insert("active_page", "channels");

    let mut type_labels = HashMap::new();
    type_labels.insert("deepseek", "DeepSeek");
    type_labels.insert("volcengine", "火山引擎");
    type_labels.insert("qwen", "阿里通义千问");
    type_labels.insert("baidu", "百度千帆");
    type_labels.insert("tencent", "腾讯混元");
    type_labels.insert("openai", "OpenAI");
    type_labels.insert("anthropic", "Anthropic");
    type_labels.insert("google", "Google Gemini");
    type_labels.insert("custom", "自定义");
    ctx.insert("channel_type_labels", &type_labels);
    render("channels.html", &ctx)
}

/// Token 管理页面 GET /admin/tokens
pub async fn tokens_page(
    State(state): State<AppState>,
) -> Result<Html<String>, AppError> {
    let mut ctx = Context::new();
    ctx.insert("title", "Token 管理");
    ctx.insert("active_page", "tokens");
    ctx.insert("port", &state.config.server.port);
    render("tokens.html", &ctx)
}

/// 充值/扣减页面 GET /admin/recharge
pub async fn recharge_page() -> Result<Html<String>, AppError> {
    let mut ctx = Context::new();
    ctx.insert("title", "积分操作");
    ctx.insert("active_page", "recharge");
    render("recharge.html", &ctx)
}

/// 流水查询页面 GET /admin/transactions
pub async fn transactions_page() -> Result<Html<String>, AppError> {
    let mut ctx = Context::new();
    ctx.insert("title", "流水查询");
    ctx.insert("active_page", "transactions");
    render("transactions.html", &ctx)
}

/// 计费设置页面 GET /admin/pricing
pub async fn pricing_page(
    State(state): State<AppState>,
) -> Result<Html<String>, AppError> {
    let multiplier = state.transaction_service.get_multiplier()?;
    let mut ctx = Context::new();
    ctx.insert("title", "计费设置");
    ctx.insert("active_page", "pricing");
    ctx.insert("multiplier", &multiplier);
    render("pricing.html", &ctx)
}

/// 渲染模板（编译时嵌入，无需外部文件）
fn render(template_name: &str, ctx: &Context) -> Result<Html<String>, AppError> {
    get_tera()
        .render(template_name, ctx)
        .map(Html)
        .map_err(|e| AppError::config_error(format!("模板渲染失败: {}", e)))
}

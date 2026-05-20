//! Web 控制台页面处理器（tera 模板渲染）

use std::collections::HashMap;
use axum::{extract::State, response::Html};
use tera::Context;

use crate::error::AppError;
use crate::state::AppState;

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

/// 渲染 tera 模板
fn render(template_name: &str, ctx: &Context) -> Result<Html<String>, AppError> {
    let template_dir = find_template_dir();
    let tera = tera::Tera::new(&format!("{}/*.html", template_dir))
        .map_err(|e| AppError::config_error(format!("模板加载失败: {}", e)))?;

    tera.render(template_name, ctx)
        .map(Html)
        .map_err(|e| AppError::config_error(format!("模板渲染失败: {}", e)))
}

fn find_template_dir() -> String {
    // 从 exe 目录向上搜索 templates/
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..5 {
            if let Some(ref d) = dir {
                let t = d.join("templates");
                if t.exists() {
                    return t.to_string_lossy().to_string();
                }
            }
            dir = dir.and_then(|d| d.parent().map(|p| p.to_path_buf()));
        }
    }
    "templates".to_string()
}

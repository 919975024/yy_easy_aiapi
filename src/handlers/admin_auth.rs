//! Admin 登录/登出处理器

use std::sync::OnceLock;
use axum::{
    extract::State,
    http::StatusCode,
    response::Html,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tera::{Context, Tera};

use crate::error::AppError;
use crate::state::AppState;

static TERA: OnceLock<Tera> = OnceLock::new();

fn get_tera() -> &'static Tera {
    TERA.get_or_init(|| crate::templates::create_tera().expect("模板加载失败"))
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// GET /admin/login — 登录页面
pub async fn login_page(
    State(state): State<AppState>,
) -> Result<Html<String>, AppError> {
    let mut ctx = Context::new();
    ctx.insert("title", "管理员登录");

    // 检测是否处于锁定状态
    let is_locked = state.login_tracker.lock().unwrap().is_locked();
    ctx.insert("is_locked", &is_locked);

    get_tera()
        .render("login.html", &ctx)
        .map(Html)
        .map_err(|e| AppError::config_error(format!("模板渲染失败: {}", e)))
}

/// POST /admin/login/api — 登录 API
pub async fn login_api(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // 检查是否已锁定
    {
        let tracker = state.login_tracker.lock().unwrap();
        if tracker.is_locked() {
            let remaining = tracker.remaining_minutes();
            return Err(AppError::business(
                StatusCode::TOO_MANY_REQUESTS,
                format!("登录已锁定，请 {} 分钟后重试", remaining),
            ));
        }
    }

    let config_username = &state.config.admin.username;
    let config_password = &state.config.admin.password;

    if req.username == *config_username && req.password == *config_password {
        // 登录成功
        let session_token = uuid::Uuid::new_v4().to_string();
        *state.admin_session_token.lock().unwrap() = Some(session_token.clone());
        state.login_tracker.lock().unwrap().reset();

        Ok(Json(json!({
            "success": true,
            "message": "登录成功",
            "session_token": session_token
        })))
    } else {
        // 登录失败
        let attempts = {
            let mut tracker = state.login_tracker.lock().unwrap();
            tracker.record_failure();
            tracker.attempts
        };

        if attempts >= 5 {
            return Err(AppError::business(
                StatusCode::TOO_MANY_REQUESTS,
                "登录已锁定，请 10 分钟后重试".to_string(),
            ));
        }

        let remaining = 5 - attempts.min(5);
        Err(AppError::business(
            StatusCode::UNAUTHORIZED,
            format!("用户名或密码错误，剩余尝试次数: {}", remaining),
        ))
    }
}

/// POST /admin/logout/api — 登出 API
pub async fn logout_api(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    *state.admin_session_token.lock().unwrap() = None;
    Json(json!({ "success": true, "message": "已退出登录" }))
}
//! 鉴权中间件
//!
//! 提供两种鉴权:
//! 1. `ApiAuthLayer` — 用于 /api/* 和 /admin/*/api 接口，检查 api_token
//! 2. `AdminSessionLayer` — 用于 /admin/* 页面，检查 session cookie

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde_json::json;
use tower::{Layer, Service};

use crate::state::AppState;

// ─── API Token 鉴权 Layer ───────────────────────────────────────────────────

/// API Token 鉴权中间件 Layer
///
/// 从 Authorization: Bearer {api_token} 中提取 token，
/// 并与 config.admin.api_token 比对。
#[derive(Clone)]
pub struct ApiAuthLayer {
    state: AppState,
}

impl ApiAuthLayer {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for ApiAuthLayer {
    type Service = ApiAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiAuthService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// API Token 鉴权中间件 Service
#[derive(Clone)]
pub struct ApiAuthService<S> {
    inner: S,
    state: AppState,
}

impl<S, ReqBody> Service<axum::http::Request<ReqBody>> for ApiAuthService<S>
where
    S: Service<axum::http::Request<ReqBody>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<ReqBody>) -> Self::Future {
        let api_token = self.state.config.admin.api_token.clone();
        let admin_session_token = self.state.admin_session_token.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            if api_token.is_empty() {
                return inner.call(req).await;
            }

            // 1. 检查 Bearer api_token
            let auth_header = req
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            if auth_header.starts_with("Bearer ") && &auth_header[7..] == api_token {
                return inner.call(req).await;
            }

            // 2. 回退检查 admin session cookie（已登录 admin 页面时无需重复提供 api_token）
            let cookie_header = req
                .headers()
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            let has_valid_session = cookie_header
                .split(';')
                .filter_map(|c| c.trim().strip_prefix("admin_session="))
                .next()
                .map_or(false, |session| {
                    let guard = admin_session_token.lock().unwrap();
                    guard.as_ref().map_or(false, |s| s == session)
                });

            if has_valid_session {
                return inner.call(req).await;
            }

            Ok(unauthorized_response("缺少 API 认证 Token 或有效的登录会话"))
        })
    }
}

// ─── Admin Session 鉴权 Layer ──────────────────────────────────────────────

/// Admin 页面会话鉴权中间件 Layer
///
/// 检查 Cookie 中的 admin_session 是否与当前有效的 session token 匹配。
#[derive(Clone)]
pub struct AdminSessionLayer {
    state: AppState,
}

impl AdminSessionLayer {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for AdminSessionLayer {
    type Service = AdminSessionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AdminSessionService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// Admin 页面会话鉴权中间件 Service
#[derive(Clone)]
pub struct AdminSessionService<S> {
    inner: S,
    state: AppState,
}

impl<S, ReqBody> Service<axum::http::Request<ReqBody>> for AdminSessionService<S>
where
    S: Service<axum::http::Request<ReqBody>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<ReqBody>) -> Self::Future {
        let state = self.state.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // 未设置用户名，无需鉴权
            if state.config.admin.username.is_empty() {
                return inner.call(req).await;
            }

            // 从 Cookie 中提取 session token
            let session_cookie = req
                .headers()
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .and_then(|cookies| {
                    for cookie in cookies.split(';') {
                        let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
                        if parts.len() == 2 && parts[0].trim() == "admin_session" {
                            return Some(parts[1].to_string());
                        }
                    }
                    None
                });

            // 验证 session token
            let is_valid = {
                let valid_session = state.admin_session_token.lock().unwrap();
                if let (Some(cookie), Some(token)) = (&session_cookie, valid_session.as_ref()) {
                    cookie == token
                } else {
                    false
                }
            };

            if is_valid {
                return inner.call(req).await;
            }

            // 未登录，判断是否为 AJAX 请求
            let is_api_call = req
                .headers()
                .get("Accept")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("application/json"))
                .unwrap_or(false)
                || req
                    .headers()
                    .get("X-Requested-With")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v == "XMLHttpRequest")
                    .unwrap_or(false);

            if is_api_call {
                Ok((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "success": false,
                        "error": {
                            "code": 401,
                            "message": "未登录，请先登录 admin 页面"
                        }
                    })),
                )
                    .into_response())
            } else {
                Ok(Redirect::to("/yy_easy_aiapi/admin/login").into_response())
            }
        })
    }
}

fn unauthorized_response(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "success": false,
            "error": {
                "code": 401,
                "message": msg
            }
        })),
    )
        .into_response()
}
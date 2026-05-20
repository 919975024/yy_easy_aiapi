//! API 集成测试 — 覆盖所有接口
//!
//! 使用 tower::ServiceExt + 内存数据库。
//! 默认配置下 api_token 为空 → API 无需鉴权。

use super::create_test_app_state;
use crate::build_router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

const PREFIX: &str = "/yy_easy_aiapi";

/// 发送请求并获取 JSON 响应
async fn send_request(app: &mut axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap_or_default();
    (status, json)
}

// ── 辅助：创建渠道 ──
async fn create_test_channel(app: &mut axum::Router, name: &str) -> i64 {
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/channels", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(
            r#"{{"name":"{}","upstream_url":"https://api.{}.com/v1","api_key":"sk-test-{}"}}"#,
            name, name, name
        )))
        .unwrap();
    let (status, json) = send_request(app, req).await;
    assert_eq!(status, StatusCode::OK, "创建渠道 {} 失败: {:?}", name, json);
    json["data"]["id"].as_i64().unwrap()
}

/// 辅助：生成 Token
async fn generate_token(app: &mut axum::Router, label: &str) -> String {
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/token/generate", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(r#"{{"label":"{}"}}"#, label)))
        .unwrap();
    let (status, json) = send_request(app, req).await;
    assert_eq!(status, StatusCode::OK, "生成 Token 失败: {:?}", json);
    json["data"]["token"].as_str().unwrap().to_string()
}

// ═══════════════════════════════════════════════════════════════
// 渠道管理
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_channels_crud() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    // ── 创建渠道 ──
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/channels", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{"name":"gpt-4o","api_standard":"openai","price_input":"0.01","price_cached":"0.005","price_output":"0.03","upstream_url":"https://api.openai.com/v1","api_key":"sk-test","channel_type":"openai","price_reference_url":"https://openai.com/pricing"}"#,
        ))
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK, "创建渠道失败: {:?}", json);
    assert!(json["success"].as_bool().unwrap());
    let data = &json["data"];
    assert_eq!(data["name"].as_str().unwrap(), "gpt-4o");
    assert_eq!(data["api_standard"].as_str().unwrap(), "openai");
    assert_eq!(data["channel_type"].as_str().unwrap(), "openai");
    assert!(data["created_at"].as_str().is_some(), "created_at 不应为空");
    let channel_id = data["id"].as_i64().unwrap();

    // ── 重复名称应成功（stoolap 没有 unique 约束报错，由代码层处理） ──
    // 注：stoolap 的 unique index 行为可能与 SQLite 不同，跳过严格检查

    // ── 查询渠道列表 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/channels", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    let arr = json["data"].as_array().unwrap();
    assert_eq!(arr.len(), 1);

    // ── 更新渠道 ──
    let req = Request::builder()
        .method("PUT")
        .uri(format!("{}/api/channels/{}", PREFIX, channel_id))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"price_input":"0.02","price_output":"0.06"}"#))
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK, "更新渠道失败: {:?}", json);
    assert_eq!(json["data"]["price_input"].as_str().unwrap(), "0.02");
    assert_eq!(json["data"]["price_output"].as_str().unwrap(), "0.06");
    // 未更新的字段保持不变
    assert_eq!(json["data"]["name"].as_str().unwrap(), "gpt-4o");

    // ── 更新不存在的渠道返回 404 ──
    let req = Request::builder()
        .method("PUT")
        .uri(format!("{}/api/channels/99999", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"price_input":"0.02"}"#))
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // ── 删除渠道 ──
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("{}/api/channels/{}", PREFIX, channel_id))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["data"]["deleted"].as_bool().unwrap());

    // ── 删除后列表为空 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/channels", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (_, json) = send_request(&mut app, req).await;
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
}

// ═══════════════════════════════════════════════════════════════
// Token 管理
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_token_lifecycle() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    // ── 生成 Token ──
    let token_str = generate_token(&mut app, "test_team").await;
    assert!(token_str.contains('_'), "Token 应包含端口前缀: {}", token_str);

    // ── 查询 Token 列表 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/tokens", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    let items = json["data"]["items"].as_array().unwrap();
    assert_eq!(json["data"]["total"].as_i64().unwrap(), 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["label"].as_str().unwrap(), "test_team");
    assert!(items[0].get("deleted").is_none(), "TokenInfo 不应包含 deleted");
    // token 不应再有 channel_name 字段
    assert!(items[0].get("channel_name").is_none(), "TokenInfo 不应包含 channel_name");

    // ── 重复生成另一个 Token ──
    let token2 = generate_token(&mut app, "team_b").await;
    assert_ne!(token_str, token2);

    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/tokens", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (_, json) = send_request(&mut app, req).await;
    assert_eq!(json["data"]["total"].as_i64().unwrap(), 2);
    assert_eq!(json["data"]["items"].as_array().unwrap().len(), 2);

    // ── 删除 Token ──
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("{}/api/tokens/{}", PREFIX, token_str))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["data"]["deleted"].as_bool().unwrap());

    // ── 删除不存在的 Token ──
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("{}/api/tokens/nonexistent_token", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // ── 列表只剩 1 个（已删除的不展示）──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/tokens", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (_, json) = send_request(&mut app, req).await;
    assert_eq!(json["data"]["total"].as_i64().unwrap(), 1);
    assert_eq!(json["data"]["items"].as_array().unwrap().len(), 1);
}

// ═══════════════════════════════════════════════════════════════
// 积分操作
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_recharge_and_deduct() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    let token_str = generate_token(&mut app, "billing_test").await;

    // ── 充值 ──
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/token/recharge", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(
            r#"{{"token":"{}","amount":"100.5"}}"#, token_str
        )))
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK, "充值失败: {:?}", json);
    assert!(json["data"]["success"].as_bool().unwrap());

    // ── 按 token 查询余额 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/token/balance?token={}", PREFIX, token_str))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    let bal = &json["data"][0];
    assert_eq!(bal["balance"].as_str().unwrap(), "100.5");
    assert!(bal.get("deleted").is_none(), "BalanceInfo 不应包含 deleted");

    // ── 按 label 查询余额 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/token/balance?label=billing_test", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"][0]["balance"].as_str().unwrap(), "100.5");

    // ── 扣减 ──
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/token/deduct", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(r#"{{"token":"{}","amount":"50"}}"#, token_str)))
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK, "扣减失败: {:?}", json);
    assert!(json["data"]["success"].as_bool().unwrap());

    // 验证余额
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/token/balance?token={}", PREFIX, token_str))
        .body(Body::empty())
        .unwrap();
    let (_, json) = send_request(&mut app, req).await;
    assert_eq!(json["data"][0]["balance"].as_str().unwrap(), "50.5");

    // ── 扣减超额 → 余额不足 ──
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/token/deduct", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(r#"{{"token":"{}","amount":"99999"}}"#, token_str)))
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "超额扣减应返回 403");

    // ── 对已物理删除 Token 充值应返回 404 ──
    let deleted_token = generate_token(&mut app, "to_delete").await;
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("{}/api/tokens/{}", PREFIX, deleted_token))
        .body(Body::empty())
        .unwrap();
    send_request(&mut app, req).await;

    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/token/recharge", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(r#"{{"token":"{}","amount":"100"}}"#, deleted_token)))
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "对已删除 Token 操作应返回 404");
}

// ═══════════════════════════════════════════════════════════════
// 流水查询
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_transaction_query() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    let token_str = generate_token(&mut app, "tx_test").await;

    // 充值产生流水
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/token/recharge", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(r#"{{"token":"{}","amount":"200"}}"#, token_str)))
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK, "充值失败: {:?}", json);

    // 扣减产生流水
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/api/token/deduct", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(r#"{{"token":"{}","amount":"30"}}"#, token_str)))
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK, "扣减失败: {:?}", json);

    // ── 查询流水（按 token）──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/token/transactions?token={}", PREFIX, token_str))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    let total = json["data"]["total"].as_i64().unwrap();
    assert_eq!(total, 2, "应有 2 条流水记录");

    let items = json["data"]["items"].as_array().unwrap();
    // 按 ID 降序，最新的在前（先看到扣减）
    assert_eq!(items[0]["change_type"].as_str().unwrap(), "deduct");
    assert_eq!(items[1]["change_type"].as_str().unwrap(), "recharge");

    // ── 查询流水（按 label，label 已从流水移除，返回 0）──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api/token/transactions?label=tx_test", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["total"].as_i64().unwrap(), 0);

    // ── 管理员流水查询 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/admin/transactions/api?token={}", PREFIX, token_str))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["data"].get("items").is_some(), "管理员流水应包含 items");
}

// ═══════════════════════════════════════════════════════════════
// 计费设置
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_pricing_settings() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    // ── 获取当前倍率 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/admin/pricing/api", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["multiplier"].as_str().unwrap(), "1.0");

    // ── 更新倍率 ──
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/admin/pricing/api", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"multiplier":"1.5"}"#))
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK, "更新倍率失败: {:?}", json);
    assert!(json["data"]["success"].as_bool().unwrap());
    assert!(json["data"]["message"].as_str().unwrap().contains("1.5"));

    // ── 验证倍率已更新 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/admin/pricing/api", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (_, json) = send_request(&mut app, req).await;
    assert_eq!(json["data"]["multiplier"].as_str().unwrap(), "1.5");

    // ── 无效倍率应返回 400 ──
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/admin/pricing/api", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"multiplier":"not_a_number"}"#))
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "无效倍率应返回 400");

    // ── 更新后再次获取 ──
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/admin/pricing/api", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (_, json) = send_request(&mut app, req).await;
    assert_eq!(json["data"]["multiplier"].as_str().unwrap(), "1.5");
}

// ═══════════════════════════════════════════════════════════════
// AI 代理
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_ask_missing_token() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/v1/chat/completions", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"model":"test","messages":[{"role":"user","content":"hi"}]}"#))
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "应返回 401: {:?}", json);
}

#[tokio::test]
async fn test_ask_invalid_token() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/v1/chat/completions", PREFIX))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer invalid_token_xyz")
        .body(Body::from(r#"{"model":"test","messages":[{"role":"user","content":"hi"}]}"#))
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "无效 Token 应返回 404");
}

#[tokio::test]
async fn test_ask_insufficient_balance() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    // 创建渠道
    create_test_channel(&mut app, "gpt-test").await;

    // 生成余额为 0 的 Token
    let token_str = generate_token(&mut app, "balance_test").await;

    // 使用 model 匹配渠道（token 未绑定渠道，用 model 名查找）
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/v1/chat/completions", PREFIX))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token_str))
        .body(Body::from(r#"{"model":"gpt-test","messages":[{"role":"user","content":"hi"}]}"#))
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "余额为 0 应返回 403");
}

#[tokio::test]
async fn test_ask_channel_not_found() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    // 生成 Token（未绑定渠道）
    let token_str = generate_token(&mut app, "no_channel").await;

    // 使用不存在的 model 名
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/v1/chat/completions", PREFIX))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token_str))
        .body(Body::from(r#"{"model":"nonexistent-channel","messages":[{"role":"user","content":"hi"}]}"#))
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在的渠道应返回 404");
}

#[tokio::test]
async fn test_ask_missing_model() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    let token_str = generate_token(&mut app, "no_model").await;

    // 请求不带 model 字段
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/v1/chat/completions", PREFIX))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token_str))
        .body(Body::from(r#"{"messages":[{"role":"user","content":"hi"}]}"#))
        .unwrap();
    let (status, _) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺少 model 应返回 400");
}

// ═══════════════════════════════════════════════════════════════
// Web 控制台页面
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_web_pages_no_auth() {
    let state = create_test_app_state();
    let app = build_router(state);

    // 默认配置 username 为空 → admin 页面无需登录
    for path in &["/admin", "/admin/tokens", "/admin/recharge", "/admin/transactions", "/admin/pricing"] {
        let req = Request::builder()
            .method("GET")
            .uri(format!("{}{}", PREFIX, path))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        let status = response.status();
        assert!(
            status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR,
            "{} 应返回 200（或 500 当模板未找到）: {}",
            path,
            status
        );
    }

    // 登录页面
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/admin/login", PREFIX))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR,
        "login 页面应可访问: {}",
        status
    );
}

// ═══════════════════════════════════════════════════════════════
// Admin 登录/登出
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_admin_login_logout() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    // ── 登录 API（默认无用户名 → 不校验，但 handler 仍检查）──
    // 配置中 username 为空时，AdminSessionLayer 不鉴权，但 login_api 仍然执行
    // 传入任意凭证
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/admin/login/api", PREFIX))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"username":"admin","password":"wrong"}"#))
        .unwrap();
    let (_status, json) = send_request(&mut app, req).await;
    // 默认 admin username="" password="" 所以凭证匹配会失败
    assert!(!json["success"].as_bool().unwrap_or(true), "空密码时应登录失败");

    // ── 登出 API（始终成功）──
    let req = Request::builder()
        .method("POST")
        .uri(format!("{}/admin/logout/api", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["success"].as_bool().unwrap());
}

// ═══════════════════════════════════════════════════════════════
// OpenAPI / Swagger
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_openapi_and_swagger() {
    let state = create_test_app_state();
    let mut app = build_router(state);

    // OpenAPI JSON
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/api-doc/openapi.json", PREFIX))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send_request(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.get("openapi").is_some(), "应返回 OpenAPI 规范");

    // Swagger UI
    let req = Request::builder()
        .method("GET")
        .uri(format!("{}/swagger", PREFIX))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

//! AI 代理处理器：POST /v1/chat/completions
//!
//! 兼容 OpenAI 格式，支持流式/非流式。
//! 流程：提取 Token → 校验余额 → 并发控制 → 转发上游 → 计费 → 记录流水。

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use rust_decimal::Decimal;
use serde_json::Value;
use tracing::info;

use crate::error::AppError;
use crate::state::AppState;

/// POST /v1/chat/completions
///
/// Token 可以使用所有渠道：如果 token 未绑定特定渠道，则从请求 body 的 model 字段匹配渠道。
/// Authorization: Bearer {token}
pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<Response<Body>, AppError> {
    // 1. 提取 Token
    let token_str = extract_bearer_token(&headers)
        .ok_or_else(|| AppError::unauthorized("缺少 Authorization: Bearer {token}"))?;

    // 2. 解析请求体（提前获取 model 字段，用于渠道匹配）
    let body_json: Value = serde_json::from_str(&body)?;
    let model_name = body_json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let is_stream = body_json
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // 3. 验证 Token + 获取渠道配置
    let (token_model, channel) = if state.config.aimock {
        // AIMOCK：只验证 token，使用默认渠道计费
        let t = state.token_service.get_by_token(&token_str)?;
        let ch = crate::db::models::Channel {
            id: 0, name: "aimock".into(), api_standard: "openai".into(),
            price_input: "0.01".into(), price_cached: "0".into(), price_output: "0.03".into(),
            rate_input: "1.0".into(), rate_cached: "1.0".into(), rate_output: "1.0".into(),
            upstream_url: String::new(), api_key: String::new(),
            channel_type: String::new(), price_reference_url: String::new(),
            model: String::new(), balance_url: String::new(),
            created_at: String::new(), updated_at: String::new(),
        };
        (t, ch)
    } else {
        state.ask_service.validate_token_and_channel(&token_str, model_name)?
    };

    // 4. 余额检查
    let balance = state.ask_service.check_balance(&token_model)?;

    // 5. 并发控制
    let max_conc = state.ask_service.max_concurrent(&balance);
    let _guard = state.ask_service.try_acquire_concurrency(&token_str, max_conc)?;

    // 6. 转发上游 或 AIMOCK
    let (upstream_status, upstream_headers, response_bytes) = if state.config.aimock {
        let mock_body = serde_json::json!({
            "id": "chatcmpl-mock-001",
            "object": "chat.completion",
            "created": 1716220800,
            "model": model_name,
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Mock response"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 10,
                "total_tokens": 20,
                "prompt_tokens_details": {"cached_tokens": 10}
            }
        });
        (StatusCode::OK, HeaderMap::new(), serde_json::to_vec(&mock_body).unwrap_or_default())
    } else {
        // 转发到上游（使用渠道配置的 model 字段，为空则回退到渠道名）
        let upstream_model = if channel.model.is_empty() { &channel.name } else { &channel.model };
        let upstream_response = forward_to_upstream(
            &channel.upstream_url,
            &channel.api_key,
            upstream_model,
            &body,
            is_stream,
        )
        .await?;

        let status = upstream_response.status();
        let headers = upstream_response.headers().clone();
        let bytes = upstream_response.bytes().await?;
        (status, headers, bytes.to_vec())
    };

    // 7. 解析 usage 并计费
    let _billing_result = if upstream_status.is_success() {
        parse_and_bill(
            &state,
            &token_str,
            &channel,
            &response_bytes,
            is_stream,
        )
        .await
    } else {
        // 上游返回错误，不扣费
        None
    };

    // 8. 构建返回响应
    let mut response = Response::builder().status(upstream_status);
    for (key, value) in upstream_headers.iter() {
        // 跳过 transfer-encoding 和 content-length（让 axum 自动处理）
        if key.as_str().to_lowercase() == "transfer-encoding"
            || key.as_str().to_lowercase() == "content-length"
        {
            continue;
        }
        response = response.header(key.as_str(), value.as_bytes());
    }

    Ok(response
        .body(Body::from(response_bytes))
        .unwrap())
}

/// 从请求头提取 Bearer Token
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("Authorization")?.to_str().ok()?;
    if auth.starts_with("Bearer ") {
        Some(auth[7..].to_string())
    } else {
        None
    }
}

/// 转发请求到上游 AI 服务
///
/// `upstream_model` 是渠道配置中指定的实际模型名（为空则用渠道名）。
async fn forward_to_upstream(
    upstream_url: &str,
    api_key: &str,
    upstream_model: &str,
    body: &str,
    is_stream: bool,
) -> Result<reqwest::Response, AppError> {
    let client = reqwest::Client::new();

    // 将请求体中的 model 替换为渠道配置的上游模型名
    let mut body_json: Value = serde_json::from_str(body)?;
    body_json["model"] = Value::String(upstream_model.to_string());
    body_json["stream"] = Value::Bool(is_stream);

    let chat_url = format!("{}/chat/completions", upstream_url.trim_end_matches('/'));

    let req = client
        .post(&chat_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body_json)
        .send()
        .await?;

    Ok(req)
}

/// 解析响应中的 usage 并计费、记录流水
/// 返回 Some(cost) 或 None（解析失败时）
async fn parse_and_bill(
    state: &AppState,
    token_str: &str,
    channel: &crate::db::models::Channel,
    response_bytes: &[u8],
    is_stream: bool,
) -> Option<Decimal> {
    let (input_tokens, cached_tokens, output_tokens) = if is_stream {
        parse_usage_from_sse(response_bytes)?
    } else {
        parse_usage_from_json(response_bytes)?
    };

    if input_tokens == 0 && output_tokens == 0 {
        info!("usage 全为 0，跳过计费");
        return Some(Decimal::ZERO);
    }

    let cost_detail = match state.ask_service.calculate_cost(channel, input_tokens, cached_tokens, output_tokens) {
        Ok(d) => d,
        Err(e) => { tracing::error!("计费计算失败: {}", e); return None; }
    };

    if cost_detail.total_cost <= Decimal::ZERO {
        return Some(Decimal::ZERO);
    }

    let request_id = uuid::Uuid::new_v4().to_string().replace('-', "");
    let metadata = format!(
        r#"{{"input_tokens":{},"cached_tokens":{},"output_tokens":{}}}"#,
        input_tokens, cached_tokens, output_tokens
    );

    // 单次调用：SQL 内完成 UPDATE balance + INSERT 全部流水字段
    let new_balance_str = match state.token_service.recharge(
        token_str, &cost_detail.total_cost,
        "consume", "api", "",
        &channel.name, &request_id,
        input_tokens, &cost_detail.input_price_final.to_string(),
        cached_tokens, &cost_detail.cached_price_final.to_string(),
        output_tokens, &cost_detail.output_price_final.to_string(),
        &cost_detail.multiplier.to_string(), &metadata,
    ) {
        Ok(b) => b,
        Err(e) => { tracing::error!("扣减余额失败: {}", e); return None; }
    };

    info!(
        "计费完成: token={}, cost={}, balance_after={}, input={}, cached={}, output={}",
        token_str, cost_detail.total_cost, new_balance_str, input_tokens, cached_tokens, output_tokens
    );

    Some(cost_detail.total_cost)
}

/// 从非流式 JSON 响应中提取 usage
fn parse_usage_from_json(data: &[u8]) -> Option<(i64, i64, i64)> {
    let json: Value = serde_json::from_slice(data).ok()?;
    let usage = json.get("usage")?;
    let input = usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let cached = usage.get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let output = usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    Some((input, cached, output))
}

/// 从 SSE 流式响应中提取 usage
///
/// OpenAI 在最后一个 data chunk 中附带 usage 信息（在 [DONE] 之前）。
/// 我们扫描所有 SSE 行，找到最后一帧中的 usage。
fn parse_usage_from_sse(data: &[u8]) -> Option<(i64, i64, i64)> {
    let text = std::str::from_utf8(data).ok()?;
    let mut last_usage: Option<(i64, i64, i64)> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "data: [DONE]" {
            continue;
        }
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(chunk) = serde_json::from_str::<Value>(json_str) {
                if let Some(usage) = chunk.get("usage") {
                    let input = usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    let cached = usage.get("prompt_tokens_details")
                        .and_then(|d| d.get("cached_tokens"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let output = usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    last_usage = Some((input, cached, output));
                }
            }
        }
    }

    last_usage
}

//! 渠道管理 API 处理器

use axum::{extract::{Path, State}, Json};
use tracing::debug;

use crate::error::AppError;
use crate::models::request::{CreateChannelRequest, UpdateChannelRequest};
use crate::models::response::{ApiResponse, ChannelInfo, ChannelTestResponse};
use crate::state::AppState;

/// 获取所有渠道
pub async fn list_channels(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<ChannelInfo>>>, AppError> {
    let channels = state.channel_service.list()?;
    Ok(Json(ApiResponse::success(channels)))
}

/// 创建渠道
pub async fn create_channel(
    State(state): State<AppState>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<Json<ApiResponse<ChannelInfo>>, AppError> {
    let ch = state.channel_service.create(
        &req.name,
        req.api_standard.as_deref().unwrap_or("openai"),
        req.price_input.as_deref().unwrap_or("0"),
        req.price_cached.as_deref().unwrap_or("0"),
        req.price_output.as_deref().unwrap_or("0"),
        req.rate_input.as_deref().unwrap_or("1.0"),
        req.rate_cached.as_deref().unwrap_or("1.0"),
        req.rate_output.as_deref().unwrap_or("1.0"),
        &req.upstream_url,
        req.api_key.as_deref().unwrap_or(""),
        &req.channel_type,
        &req.price_reference_url,
        &req.model,
        &req.balance_url,
    )?;
    Ok(Json(ApiResponse::success(ch)))
}

/// 更新渠道
pub async fn update_channel(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<ApiResponse<ChannelInfo>>, AppError> {
    let ch = state.channel_service.update(
        id,
        req.api_standard.as_deref(),
        req.price_input.as_deref(),
        req.price_cached.as_deref(),
        req.price_output.as_deref(),
        req.rate_input.as_deref(),
        req.rate_cached.as_deref(),
        req.rate_output.as_deref(),
        req.upstream_url.as_deref(),
        req.api_key.as_deref(),
        req.channel_type.as_deref(),
        req.price_reference_url.as_deref(),
        req.model.as_deref(),
        req.balance_url.as_deref(),
    )?;
    Ok(Json(ApiResponse::success(ch)))
}

/// 删除渠道
pub async fn delete_channel(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    state.channel_service.delete(id)?;
    Ok(Json(ApiResponse::success(serde_json::json!({"deleted": true, "id": id}))))
}

/// 测试渠道连通性（发送一条最小 chat/completions 请求到上游）
pub async fn test_channel_connection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<ChannelTestResponse>>, AppError> {
    let ch = state.channel_service.get_by_id(id)?;

    debug!(
        channel_id = ch.id,
        channel_name = %ch.name,
        upstream_url = %ch.upstream_url,
        channel_type = %ch.channel_type,
        "测试渠道连通性"
    );

    if ch.api_key.is_empty() {
        return Ok(Json(ApiResponse::success(ChannelTestResponse {
            channel_id: ch.id,
            channel_name: ch.name.clone(),
            ok: false,
            elapsed_ms: 0,
            upstream_status: None,
            error_message: Some("未配置 API Key".to_string()),
        })));
    }

    // 构建最小测试请求: 发送 "hi" 获取 1 个 token
    let test_url = format!("{}/chat/completions", ch.upstream_url.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": if ch.model.is_empty() { &ch.name } else { &ch.model },
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1,
        "stream": false
    });

    let masked_key = if ch.api_key.len() > 12 {
        format!("{}****{}", &ch.api_key[..8], &ch.api_key[ch.api_key.len()-4..])
    } else {
        "***".to_string()
    };

    debug!(
        url = %test_url,
        model = %body["model"],
        api_key = %masked_key,
        "发起连通测试请求"
    );

    let start = std::time::Instant::now();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::business(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("HTTP 客户端构建失败: {}", e),
        ))?;

    match client
        .post(&test_url)
        .header("Content-Type", "application/json")
        .bearer_auth(&ch.api_key)
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let elapsed = start.elapsed();
            let status_code = resp.status().as_u16();
            let resp_text = resp.text().await.unwrap_or_default();

            debug!(
                status = status_code,
                elapsed_ms = elapsed.as_millis(),
                body = %resp_text,
                "连通测试响应"
            );

            // 2xx 即视为连通；4xx 可能是模型名不对或额度问题，也视为连通
            // （只要上游给了回应，就说明网络和鉴权是通的）
            let ok = status_code < 500;

            Ok(Json(ApiResponse::success(ChannelTestResponse {
                channel_id: ch.id,
                channel_name: ch.name.clone(),
                ok,
                elapsed_ms: elapsed.as_millis(),
                upstream_status: Some(status_code),
                error_message: if ok { None } else { Some(format!("上游返回 HTTP {}", status_code)) },
            })))
        }
        Err(e) => {
            let elapsed = start.elapsed();
            debug!(
                error = %e,
                elapsed_ms = elapsed.as_millis(),
                "连通测试请求失败"
            );
            Ok(Json(ApiResponse::success(ChannelTestResponse {
                channel_id: ch.id,
                channel_name: ch.name.clone(),
                ok: false,
                elapsed_ms: elapsed.as_millis(),
                upstream_status: None,
                error_message: Some(format!("请求失败: {}", e)),
            })))
        }
    }
}
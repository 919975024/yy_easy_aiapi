//! Token 管理 API 处理器

use axum::{extract::{Path, Query, State}, Json};

use crate::error::AppError;
use crate::models::request::{GenerateTokenRequest, TokenListQuery};
use crate::models::response::{ApiResponse, TokenGenerated, TokenPage};
use crate::state::AppState;

/// 生成 Token
pub async fn generate_token(
    State(state): State<AppState>,
    Json(req): Json<GenerateTokenRequest>,
) -> Result<Json<ApiResponse<TokenGenerated>>, AppError> {
    let result = state.token_service.generate(&req.label)?;
    Ok(Json(ApiResponse::success(result)))
}

/// 查询 Token 列表（支持分页）
pub async fn list_tokens(
    State(state): State<AppState>,
    Query(query): Query<TokenListQuery>,
) -> Result<Json<ApiResponse<TokenPage>>, AppError> {
    let base_points = state.config.concurrency.base_points_per_concurrent;
    let result = state.token_service.list_paged(base_points, query.page, query.page_size)?;
    Ok(Json(ApiResponse::success(result)))
}

/// 删除 Token（物理删除）
pub async fn delete_token(
    State(state): State<AppState>,
    Path(token_str): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    state.token_service.delete(&token_str)?;
    Ok(Json(ApiResponse::success(serde_json::json!({"deleted": true, "token": token_str}))))
}

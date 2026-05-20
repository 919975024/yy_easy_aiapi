//! 计费设置 API 处理器

use axum::{extract::State, Json};
use serde::Deserialize;

use crate::error::AppError;
use crate::models::response::{ApiResponse, OperationResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdatePricingRequest {
    pub multiplier: String,
}

/// 获取当前倍率 GET /admin/pricing/api
pub async fn get_pricing(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let multiplier = state.transaction_service.get_multiplier()?;
    Ok(Json(ApiResponse::success(serde_json::json!({
        "multiplier": multiplier
    }))))
}

/// 更新倍率 POST /admin/pricing/api
pub async fn update_pricing(
    State(state): State<AppState>,
    Json(req): Json<UpdatePricingRequest>,
) -> Result<Json<ApiResponse<OperationResult>>, AppError> {
    // 验证 multiplier 是有效的数字
    let _: rust_decimal::Decimal = req.multiplier.parse().map_err(|_| {
        AppError::business(axum::http::StatusCode::BAD_REQUEST, "倍率格式无效，请输入有效数字")
    })?;

    state.transaction_service.set_multiplier(&req.multiplier)?;
    Ok(Json(ApiResponse::success(OperationResult {
        success: true,
        message: format!("倍率已更新为 {}", req.multiplier),
    })))
}
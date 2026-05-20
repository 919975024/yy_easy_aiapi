//! 积分管理 API 处理器：充值、扣减、余额查询

use axum::{extract::{Query, State}, Json};
use rust_decimal::Decimal;

use crate::error::AppError;
use crate::models::request::{BalanceQuery, DeductRequest, RechargeRequest};
use crate::models::response::{ApiResponse, BalanceInfo, OperationResult};
use crate::state::AppState;

/// 充值
pub async fn recharge(
    State(state): State<AppState>,
    Json(req): Json<RechargeRequest>,
) -> Result<Json<ApiResponse<OperationResult>>, AppError> {
    let amount: Decimal = req.amount.parse().map_err(|_| {
        AppError::business(axum::http::StatusCode::BAD_REQUEST, "金额格式错误")
    })?;

    let new_balance = state.token_service.recharge(
        &req.token, &amount,
        "recharge", "admin", &req.remark,
        "", "", 0, "0", 0, "0", 0, "0", "1.0", "",
    )?;

    Ok(Json(ApiResponse::success(OperationResult {
        success: true,
        message: format!("充值成功，当前余额: {}", new_balance),
    })))
}

/// 扣减
pub async fn deduct(
    State(state): State<AppState>,
    Json(req): Json<DeductRequest>,
) -> Result<Json<ApiResponse<OperationResult>>, AppError> {
    let amount: Decimal = req.amount.parse().map_err(|_| {
        AppError::business(axum::http::StatusCode::BAD_REQUEST, "金额格式错误")
    })?;

    let old_balance_f64 = state.token_service.get_balance_f64(&req.token)?;
    let old_balance: Decimal = old_balance_f64
        .to_string()
        .parse()
        .map_err(|_| AppError::business(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "余额解析失败"))?;

    if amount > old_balance {
        return Err(AppError::insufficient_balance(format!(
            "余额不足，当前余额 {}，扣减 {}", old_balance, amount
        )));
    }

    let new_balance = state.token_service.recharge(
        &req.token, &amount,
        "deduct", "admin", &req.remark,
        "", "", 0, "0", 0, "0", 0, "0", "1.0", "",
    )?;

    Ok(Json(ApiResponse::success(OperationResult {
        success: true,
        message: format!("扣减成功，当前余额: {}", new_balance),
    })))
}

/// 余额查询
pub async fn get_balance(
    State(state): State<AppState>,
    Query(query): Query<BalanceQuery>,
) -> Result<Json<ApiResponse<Vec<BalanceInfo>>>, AppError> {
    let bp = state.config.concurrency.base_points_per_concurrent;

    let results = if let Some(ref token_str) = query.token {
        if !token_str.is_empty() {
            vec![state.token_service.get_balance(token_str, bp)?]
        } else if let Some(ref label) = query.label {
            state.token_service.get_balance_by_label(label, bp)?
        } else {
            return Err(AppError::business(
                axum::http::StatusCode::BAD_REQUEST,
                "请提供 token 或 label 参数",
            ));
        }
    } else if let Some(ref label) = query.label {
        state.token_service.get_balance_by_label(label, bp)?
    } else {
        return Err(AppError::business(
            axum::http::StatusCode::BAD_REQUEST,
            "请提供 token 或 label 参数",
        ));
    };

    Ok(Json(ApiResponse::success(results)))
}

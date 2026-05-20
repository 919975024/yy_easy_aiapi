//! 流水查询 API 处理器

use axum::{extract::{Query, State}, Json};

use crate::error::AppError;
use crate::models::request::TransactionQuery;
use crate::models::response::{AdminTransactionPage, ApiResponse, TransactionPage};
use crate::state::AppState;

/// 分页查询流水（非管理员，只返回基本字段）
pub async fn list_transactions(
    State(state): State<AppState>,
    Query(query): Query<TransactionQuery>,
) -> Result<Json<ApiResponse<TransactionPage>>, AppError> {
    let page = query.page.max(1);
    let page_size = query.page_size.min(100).max(1);

    let result = state.transaction_service.query(
        query.token.as_deref(),
        query.label.as_deref(),
        query.start_time.as_deref(),
        query.end_time.as_deref(),
        page,
        page_size,
    )?;

    Ok(Json(ApiResponse::success(result)))
}

/// 分页查询流水（管理员，含所有计费详情字段）
pub async fn admin_list_transactions(
    State(state): State<AppState>,
    Query(query): Query<TransactionQuery>,
) -> Result<Json<ApiResponse<AdminTransactionPage>>, AppError> {
    let page = query.page.max(1);
    let page_size = query.page_size.min(100).max(1);

    let result = state.transaction_service.admin_query(
        query.token.as_deref(),
        query.label.as_deref(),
        query.start_time.as_deref(),
        query.end_time.as_deref(),
        page,
        page_size,
    )?;

    Ok(Json(ApiResponse::success(result)))
}
//! 请求日志中间件

use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;
use tracing::info;

/// 记录每个请求的 METHOD、URI、状态码、耗时
pub async fn logging_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req
        .uri()
        .path_and_query()
        .map(|u| u.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed();
    let status = response.status();

    info!(
        "{} {} → {} ({:.2}ms)",
        method,
        uri,
        status.as_u16(),
        elapsed.as_secs_f64() * 1000.0
    );

    response
}

use tracing::info;

use yy_easy_aiapi::config::AppConfig;
use yy_easy_aiapi::{build_app_state, build_router, init_logging};

#[tokio::main]
async fn main() {
    let config = AppConfig::from_file();

    init_logging(config.logging.debug);

    info!("yy_easy_aiapi — AI 代理网关 启动中...");
    info!(
        "配置加载完成: host={}, port={}, db={}",
        config.server.host, config.server.port, config.database.url
    );

    let state = build_app_state(config.clone());
    let app = build_router(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect(&format!("无法绑定到地址: {}", addr));

    let port = listener.local_addr().unwrap().port();

    info!("============================================");
    info!("  yy_easy_aiapi 启动成功！");
    info!("  AI 代理:     http://127.0.0.1:{}/yy_easy_aiapi/v1/chat/completions", port);
    info!("  管理 API:    http://127.0.0.1:{}/yy_easy_aiapi/api/", port);
    info!("  Web 控制台:  http://127.0.0.1:{}/yy_easy_aiapi/admin", port);
    info!("  Swagger 文档: http://127.0.0.1:{}/yy_easy_aiapi/swagger", port);
    info!("============================================");

    axum::serve(listener, app)
        .await
        .expect("服务器启动失败");
}

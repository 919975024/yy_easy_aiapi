//! 测试通用工具

/// 测试配置初始化
#[allow(dead_code)]
pub fn setup() {
    // 初始化日志（测试时不输出）
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
}

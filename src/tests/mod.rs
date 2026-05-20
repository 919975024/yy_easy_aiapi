//! 集成测试模块

use std::sync::atomic::{AtomicU32, Ordering};
use crate::config::AppConfig;
use crate::db::init_database;
use crate::state::AppState;

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

/// 创建测试用的 AppState（每个测试用独立内存数据库）
pub fn create_test_app_state() -> AppState {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let config = AppConfig::default();
    let db_path = format!("memory://test_{}", id);
    let db = init_database(&db_path).expect("测试数据库初始化失败");
    AppState::new(db, config)
}

#[cfg(test)]
mod api_test;

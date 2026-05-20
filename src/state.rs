//! 全局应用状态
//!
//! 通过 Axum 的 State extractor 注入到每个 handler 中。

use std::sync::{Arc, Mutex};

use stoolap::api::Database;

use crate::config::AppConfig;
use crate::services::ask::AskService;
use crate::services::channel::ChannelService;
use crate::services::token::TokenService;
use crate::services::transaction::TransactionService;
use crate::snowflake::IdGenerator;

/// admin 登录锁定追踪器
pub struct LoginTracker {
    pub attempts: u32,
    pub locked_until: Option<chrono::DateTime<chrono::Utc>>,
}

impl LoginTracker {
    pub fn new() -> Self {
        Self { attempts: 0, locked_until: None }
    }

    /// 记录一次失败登录，达到 5 次则锁定 10 分钟
    pub fn record_failure(&mut self) {
        self.attempts += 1;
        if self.attempts >= 5 {
            self.locked_until = Some(chrono::Utc::now() + chrono::Duration::minutes(10));
        }
    }

    /// 是否处于锁定状态
    pub fn is_locked(&self) -> bool {
        if let Some(until) = self.locked_until {
            if chrono::Utc::now() < until {
                return true;
            }
        }
        false
    }

    /// 重置追踪器（登录成功时调用）
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.locked_until = None;
    }

    /// 锁定剩余分钟数
    pub fn remaining_minutes(&self) -> i64 {
        self.locked_until
            .map(|t| (t - chrono::Utc::now()).num_minutes() + 1)
            .unwrap_or(0)
            .max(0)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: AppConfig,
    pub id_gen: Arc<IdGenerator>,
    pub channel_service: ChannelService,
    pub token_service: TokenService,
    pub transaction_service: TransactionService,
    pub ask_service: AskService,
    pub admin_session_token: Arc<Mutex<Option<String>>>,
    pub login_tracker: Arc<Mutex<LoginTracker>>,
}

impl AppState {
    pub fn new(db: Database, config: AppConfig) -> Self {
        let db = Arc::new(db);
        let id_gen = Arc::new(IdGenerator::new());
        let channel_service = ChannelService::new(db.clone(), id_gen.clone());
        let token_service = TokenService::new(db.clone(), config.server.port);
        let transaction_service = TransactionService::new(db.clone());
        let ask_service = AskService::new(
            db.clone(),
            config.concurrency.base_points_per_concurrent,
            config.pricing.decimal_places,
            config.server.port,
        );

        Self {
            db,
            config,
            id_gen,
            channel_service,
            token_service,
            transaction_service,
            ask_service,
            admin_session_token: Arc::new(Mutex::new(None)),
            login_tracker: Arc::new(Mutex::new(LoginTracker::new())),
        }
    }
}

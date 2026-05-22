//! Token 管理服务
//!
//! Token 格式：{port}_{uuid}，例如 8080_a1b2c3d4e5f6

use std::sync::{Arc, Mutex};
use dashmap::DashMap;
use rust_decimal::prelude::ToPrimitive;
use stoolap::api::Database;
use uuid::Uuid;

use crate::db::models::Token;
use crate::error::AppError;
use crate::models::response::{BalanceInfo, TokenGenerated, TokenInfo, TokenPage};

#[derive(Clone)]
pub struct TokenService {
    db: Arc<Database>,
    port: u16,
    token_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl TokenService {
    pub fn new(db: Arc<Database>, port: u16) -> Self {
        Self { db, port, token_locks: Arc::new(DashMap::new()) }
    }

    /// 生成新 Token（格式：{port}_{uuid}）
    pub fn generate(&self, label: &str) -> Result<TokenGenerated, AppError> {
        let uuid_str = Uuid::new_v4().to_string().replace('-', "");
        let token_str = format!("{}_{}", self.port, uuid_str);
        let balance = 0f64;

        let now = crate::now_iso();
        self.db.execute(
            "INSERT INTO tokens (token, label, balance, created_at) VALUES ($1, $2, $3, $4)",
            (token_str.clone(), label.to_string(), balance, now),
        )?;

        Ok(TokenGenerated {
            token: token_str,
            label: label.to_string(),
            balance: balance.to_string(),
        })
    }

    /// 查询所有 Token
    pub fn list(&self, base_points: i64) -> Result<Vec<TokenInfo>, AppError> {
        let tokens: Vec<Token> = self.db.query_as(
            "SELECT id, token, label, balance, created_at FROM tokens ORDER BY id DESC",
            (),
        )?;
        Ok(tokens.into_iter().map(|t| token_to_info(t, base_points)).collect())
    }

    /// 分页查询 Token
    pub fn list_paged(&self, base_points: i64, page: i64, page_size: i64) -> Result<TokenPage, AppError> {
        let total: i64 = {
            let result = self.db.query(
                "SELECT COUNT(*) FROM tokens",
                (),
            )?;
            if let Some(Ok(row)) = result.into_iter().next() {
                row.get::<i64>(0).unwrap_or(0)
            } else {
                0
            }
        };

        let offset = (page - 1) * page_size;
        let sql = format!(
            "SELECT id, token, label, balance, created_at FROM tokens ORDER BY id DESC LIMIT {} OFFSET {}",
            page_size, offset,
        );
        let tokens: Vec<Token> = self.db.query_as(&sql, ())?;
        let items: Vec<TokenInfo> = tokens.into_iter().map(|t| token_to_info(t, base_points)).collect();

        Ok(TokenPage { items, total, page, page_size })
    }

    /// 底层余额查询：仅查询 balance 字段（REAL → f64）
    ///
    /// 这是查询余额的底层方法，只执行 `SELECT balance FROM tokens WHERE token = $1`，
    /// 不查询其他任何字段。适合仅需余额数值的场景（费用计算、余额检查等）。
    pub fn get_balance_f64(&self, token_str: &str) -> Result<f64, AppError> {
        self.db.query(
            "SELECT balance FROM tokens WHERE token = $1",
            (token_str.to_string(),),
        )?.into_iter().next()
            .and_then(|r| r.ok())
            .and_then(|row| row.get::<f64>(0).ok())
            .ok_or_else(|| AppError::not_found(format!("Token 不存在: {}", token_str)))
    }

    /// 按 token 字符串查询
    pub fn get_by_token(&self, token_str: &str) -> Result<Token, AppError> {
        let rows: Vec<Token> = self.db.query_as(
            "SELECT id, token, label, balance, created_at FROM tokens WHERE token = $1",
            (token_str.to_string(),),
        )?;
        rows.into_iter().next()
            .ok_or_else(|| AppError::not_found(format!("Token 不存在: {}", token_str)))
    }

    /// 按 label 查询
    pub fn get_by_label(&self, label: &str) -> Result<Vec<Token>, AppError> {
        let rows: Vec<Token> = self.db.query_as(
            "SELECT id, token, label, balance, created_at FROM tokens WHERE label = $1",
            (label.to_string(),),
        )?;
        Ok(rows)
    }

    /// 物理删除 Token
    pub fn delete(&self, token_str: &str) -> Result<(), AppError> {
        self.get_by_token(token_str)?;
        self.db.execute(
            "DELETE FROM tokens WHERE token = $1",
            (token_str.to_string(),),
        )?;
        Ok(())
    }

    /// 余额变动（统一充值/扣减/AI消费）
    ///
    /// SQL 统一为 `balance = balance + $1`：正数=充值，负数=扣减。
    /// change_type / operator / 计费字段由调用方传入：
    /// - 充值: change_type="recharge", operator="admin", amount>0, 计费字段 0/空
    /// - 扣减: change_type="deduct",   operator="admin", amount>0（方法内取反）, 计费字段 0/空
    /// - AI消费: change_type="consume", operator="api", amount>0（方法内取反）, 计费字段实际值
    pub fn recharge(
        &self, token_str: &str, amount: &rust_decimal::Decimal,
        change_type: &str, operator: &str, remark: &str,
        channel_name: &str, request_id: &str,
        input_tokens: i64, input_price: &str,
        cached_tokens: i64, cached_price: &str,
        output_tokens: i64, output_price: &str,
        multiplier: &str, metadata: &str,
    ) -> Result<String, AppError> {
        use rust_decimal::prelude::ToPrimitive;

        // 相同 token 串行化，防并发写导致余额错乱
        let lock = self.token_locks
            .entry(token_str.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

        let delta = match change_type {
            "deduct" | "consume" => -amount,
            _ => *amount,
        };
        let delta_f64 = delta.to_f64().unwrap_or(0.0);

        let f = |s: &str| s.parse::<f64>().unwrap_or(0.0);
        let prefix = token_str.split('_').next().unwrap_or("");
        let now = crate::now_iso();
        let remark = if remark.is_empty() { "管理员操作" } else { remark }.to_string();

        // Step 0: 查询当前余额（持锁内，防并发）
        let t0 = std::time::Instant::now();
        let old_balance_f64 = {
            let r = self.db.query(
                "SELECT balance FROM tokens WHERE token = $1",
                (token_str.to_string(),),
            )?;
            r.into_iter().next()
                .and_then(|r| r.ok())
                .and_then(|row| row.get::<f64>(0).ok())
                .unwrap_or(0.0)
        };
        let new_balance_f64 = old_balance_f64 + delta_f64;
        let t_read = t0.elapsed();

        // Step 1: UPDATE balance（直接写入新值，避免读-改-写）
        let t2 = std::time::Instant::now();
        self.db.execute(
            "UPDATE tokens SET balance = $1 WHERE token = $2",
            (new_balance_f64, token_str.to_string()),
        )?;
        let t_update = t2.elapsed();

        // Step 2: INSERT 流水
        let t3 = std::time::Instant::now();
        let esc = |s: &str| s.replace('\'', "''");
        let sql = format!(
            "INSERT INTO transactions (token, token_prefix, change_type, amount, \
             balance_after, channel_name, request_id, operator, remark, created_at, \
             input_tokens, input_price, cached_tokens, cached_price, \
             output_tokens, output_price, multiplier, metadata) \
             VALUES ($1,$2,'{}',$3, \
             {}, \
             $4,$5,'{}',$6,$7, \
             {},{},{},{},{},{},{},'{}')",
            esc(change_type),
            new_balance_f64,
            esc(operator),
            input_tokens, f(input_price),
            cached_tokens, f(cached_price),
            output_tokens, f(output_price),
            f(multiplier),
            esc(metadata),
        );

        self.db.execute(
            &sql,
            (
                token_str.to_string(),      // $1
                prefix.to_string(),         // $2
                delta_f64,                  // $3
                channel_name.to_string(),   // $4
                request_id.to_string(),     // $5
                remark,                     // $6
                now,                        // $7
            ),
        )?;
        let t_insert = t3.elapsed();

        tracing::info!("recharge READ {:?}  UPDATE {:?}  INSERT {:?}  new_balance={}  token={}", t_read, t_update, t_insert, new_balance_f64, token_str);

        Ok(new_balance_f64.to_string())
    }

    /// 查询余额（仅查询 token/label/balance 三个字段）
    pub fn get_balance(&self, token_str: &str, base_points: i64) -> Result<BalanceInfo, AppError> {
        let row = self.db.query(
            "SELECT token, label, balance FROM tokens WHERE token = $1",
            (token_str.to_string(),),
        )?.into_iter().next()
            .and_then(|r| r.ok())
            .ok_or_else(|| AppError::not_found(format!("Token 不存在: {}", token_str)))?;

        let token: String = row.get(0).ok().unwrap_or_default();
        let label: String = row.get(1).ok().unwrap_or_default();
        let balance_f64: f64 = row.get(2).ok().unwrap_or(0.0);
        let balance_str = balance_f64.to_string();

        let bal: rust_decimal::Decimal = balance_str.parse().unwrap_or_default();
        let max_concurrent = if base_points > 0 {
            (bal / rust_decimal::Decimal::from(base_points)).floor().to_i64().unwrap_or(0)
        } else {
            0
        };

        Ok(BalanceInfo {
            token,
            label,
            balance: balance_str,
            max_concurrent,
        })
    }

    /// 按 label 查询余额列表
    pub fn get_balance_by_label(&self, label: &str, base_points: i64) -> Result<Vec<BalanceInfo>, AppError> {
        let tokens = self.get_by_label(label)?;
        tokens.into_iter().map(|t| {
            let bal: rust_decimal::Decimal = t.balance.parse().unwrap_or_default();
            let max_concurrent = if base_points > 0 {
                (bal / rust_decimal::Decimal::from(base_points)).floor().to_i64().unwrap_or(0)
            } else { 0 };
            Ok(BalanceInfo {
                token: t.token,
                label: t.label,
                balance: t.balance,
                max_concurrent,
            })
        }).collect()
    }
}

fn token_to_info(t: Token, base_points: i64) -> TokenInfo {
    let bal: rust_decimal::Decimal = t.balance.parse().unwrap_or_default();
    let max_concurrent = if base_points > 0 {
        (bal / rust_decimal::Decimal::from(base_points)).floor().to_i64().unwrap_or(0)
    } else {
        0
    };

    TokenInfo {
        id: t.id,
        token: t.token,
        label: t.label,
        balance: t.balance,
        created_at: t.created_at,
        max_concurrent,
    }
}

//! 流水审计服务

use std::sync::Arc;
use stoolap::api::Database;

use crate::db::models::Transaction;
use crate::error::AppError;
use crate::models::response::{AdminTransactionInfo, AdminTransactionPage, TransactionInfo, TransactionPage};

#[derive(Clone)]
pub struct TransactionService {
    db: Arc<Database>,
}

impl TransactionService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn get_multiplier(&self) -> Result<String, AppError> {
        let rows = self.db.query(
            "SELECT value FROM settings WHERE name = 'price_multiplier'",
            (),
        )?;
        if let Some(Ok(row)) = rows.into_iter().next() {
            Ok(row.get::<String>(0).unwrap_or_else(|_| "1.0".to_string()))
        } else {
            Ok("1.0".to_string())
        }
    }

    pub fn set_multiplier(&self, multiplier: &str) -> Result<(), AppError> {
        // 先确保行存在（stoolap 可能不支持 INSERT OR REPLACE/IGNORE）
        let existing = self.db.query(
            "SELECT 1 FROM settings WHERE name = 'price_multiplier'",
            (),
        )?.into_iter().next();

        if existing.is_some() {
            self.db.execute(
                "UPDATE settings SET value = $1 WHERE name = 'price_multiplier'",
                (multiplier.to_string(),),
            )?;
        } else {
            self.db.execute(
                "INSERT INTO settings (id, name, value) VALUES (1, 'price_multiplier', $1)",
                (multiplier.to_string(),),
            )?;
        }
        Ok(())
    }

    /// 分页查询流水（非管理员，只返回基本信息）
    pub fn query(
        &self,
        token: Option<&str>,
        label: Option<&str>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<TransactionPage, AppError> {
        let mut conditions: Vec<String> = vec![];

        let esc = |s: &str| s.replace('\'', "''");

        if let Some(t) = token {
            if !t.is_empty() {
                conditions.push(format!("token = '{}'", esc(t)));
            }
        }
        if let Some(l) = label {
            if !l.is_empty() {
                conditions.push(format!("label = '{}'", esc(l)));
            }
        }
        if let Some(start) = start_time {
            if !start.is_empty() {
                conditions.push(format!("created_at >= '{}'", esc(start)));
            }
        }
        if let Some(end) = end_time {
            if !end.is_empty() {
                conditions.push(format!("created_at <= '{}'", esc(end)));
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM transactions {}", where_clause);
        let total_result = self.db.query(&count_sql, ())?;
        let total: i64 = if let Some(Ok(row)) = total_result.into_iter().next() {
            row.get::<i64>(0).unwrap_or(0)
        } else {
            0
        };

        let offset = (page - 1) * page_size;
        let data_sql = format!(
            "SELECT id, token, token_prefix, label, change_type, amount, balance_after, channel_name, request_id, operator, metadata, remark, created_at, input_tokens, input_price, cached_tokens, cached_price, output_tokens, output_price, multiplier FROM transactions {} ORDER BY id DESC LIMIT {} OFFSET {}",
            where_clause, page_size, offset,
        );

        let rows: Vec<Transaction> = self.db.query_as(&data_sql, ())?;
        let items: Vec<TransactionInfo> = rows.into_iter().map(|t| TransactionInfo {
            id: t.id,
            token: t.token,
            token_prefix: t.token_prefix,
            label: t.label,
            change_type: t.change_type,
            amount: t.amount,
            balance_after: t.balance_after,
            channel_name: t.channel_name,
            request_id: t.request_id,
            operator: t.operator,
            metadata: t.metadata,
            remark: t.remark,
            created_at: t.created_at,
        }).collect();

        Ok(TransactionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    /// 分页查询流水（管理员，含所有计费详情字段）
    pub fn admin_query(
        &self,
        token: Option<&str>,
        label: Option<&str>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<AdminTransactionPage, AppError> {
        let mut conditions: Vec<String> = vec![];

        let esc = |s: &str| s.replace('\'', "''");

        if let Some(t) = token {
            if !t.is_empty() {
                conditions.push(format!("token = '{}'", esc(t)));
            }
        }
        if let Some(l) = label {
            if !l.is_empty() {
                conditions.push(format!("label = '{}'", esc(l)));
            }
        }
        if let Some(start) = start_time {
            if !start.is_empty() {
                conditions.push(format!("created_at >= '{}'", esc(start)));
            }
        }
        if let Some(end) = end_time {
            if !end.is_empty() {
                conditions.push(format!("created_at <= '{}'", esc(end)));
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM transactions {}", where_clause);
        let total_result = self.db.query(&count_sql, ())?;
        let total: i64 = if let Some(Ok(row)) = total_result.into_iter().next() {
            row.get::<i64>(0).unwrap_or(0)
        } else {
            0
        };

        let offset = (page - 1) * page_size;
        let data_sql = format!(
            "SELECT id, token, token_prefix, label, change_type, amount, balance_after, channel_name, request_id, operator, metadata, remark, created_at, input_tokens, input_price, cached_tokens, cached_price, output_tokens, output_price, multiplier FROM transactions {} ORDER BY id DESC LIMIT {} OFFSET {}",
            where_clause, page_size, offset,
        );

        let rows: Vec<Transaction> = self.db.query_as(&data_sql, ())?;
        let items: Vec<AdminTransactionInfo> = rows.into_iter().map(|t| AdminTransactionInfo {
            id: t.id,
            token: t.token,
            token_prefix: t.token_prefix,
            label: t.label,
            change_type: t.change_type,
            amount: t.amount,
            balance_after: t.balance_after,
            channel_name: t.channel_name,
            request_id: t.request_id,
            operator: t.operator,
            metadata: t.metadata,
            remark: t.remark,
            created_at: t.created_at,
            input_tokens: t.input_tokens,
            input_price: t.input_price,
            cached_tokens: t.cached_tokens,
            cached_price: t.cached_price,
            output_tokens: t.output_tokens,
            output_price: t.output_price,
            multiplier: t.multiplier,
        }).collect();

        Ok(AdminTransactionPage {
            items,
            total,
            page,
            page_size,
        })
    }
}
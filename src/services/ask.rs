//! AI 代理服务：Token 校验、余额检查、并发控制、上游转发、计费、流水记录
//!
//! 并发控制：max_concurrent = balance / base_points（整除），通过 DashMap 计数器实现。
//! 计费：final_price = channel_price × multiplier × 100
//!       cost = sum(final_price_i * tokens_i / 1000)，ceil 到 4 位小数。

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::http::StatusCode;
use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use stoolap::api::Database;

use crate::db::models::{Channel, Token};
use crate::error::AppError;
use crate::services::token::TokenService;
use crate::services::transaction::TransactionService;

/// 并发计数器：track 每个 token 当前活跃请求数
type ConcurrencyMap = Arc<DashMap<String, Arc<AtomicI64>>>;

/// 计费详情：cost + 中间价格，供 handler 传给 TokenService::consume 写入流水
pub struct CostDetail {
    pub total_cost: Decimal,
    pub input_price_final: Decimal,
    pub cached_price_final: Decimal,
    pub output_price_final: Decimal,
    pub multiplier: Decimal,
}

#[derive(Clone)]
pub struct AskService {
    db: Arc<Database>,
    base_points: i64,
    decimal_places: u32,
    #[allow(dead_code)]
    port: u16,
    concurrency: ConcurrencyMap,
    token_service: TokenService,
    transaction_service: TransactionService,
}

impl AskService {
    pub fn new(db: Arc<Database>, base_points: i64, decimal_places: u32, port: u16) -> Self {
        Self {
            concurrency: Arc::new(DashMap::new()),
            token_service: TokenService::new(db.clone(), port),
            transaction_service: TransactionService::new(db.clone()),
            db,
            base_points,
            decimal_places,
            port,
        }
    }

    /// 获取 token 的端口前缀（辅助函数）
    pub fn token_prefix(token: &str) -> String {
        token.split('_').next().unwrap_or("").to_string()
    }

    /// 验证 token 并获取渠道配置
    ///
    /// Token 可使用所有渠道，渠道由请求 body 中的 model 字段动态匹配。
    /// 返回 (Token, Channel)
    pub fn validate_token_and_channel(
        &self,
        token_str: &str,
        model_name: &str,
    ) -> Result<(Token, Channel), AppError> {
        let t = self.token_service.get_by_token(token_str)?;

        if model_name.is_empty() {
            return Err(AppError::business(
                StatusCode::BAD_REQUEST,
                "请求中未指定 model",
            ));
        }

        let channels: Vec<Channel> = self.db.query_as(
            "SELECT id, name, api_standard, price_input, price_cached, price_output, rate_input, rate_cached, rate_output, upstream_url, api_key, channel_type, price_reference_url, model, balance_url, created_at, updated_at FROM channels WHERE name = $1",
            (model_name.to_string(),),
        )?;

        let ch = channels.into_iter().next()
            .ok_or_else(|| AppError::not_found(format!("渠道不存在: {}", model_name)))?;

        Ok((t, ch))
    }

    /// 检查余额
    pub fn check_balance(&self, t: &Token) -> Result<Decimal, AppError> {
        let balance: Decimal = t.balance.parse().map_err(|_| {
            AppError::business(StatusCode::INTERNAL_SERVER_ERROR, "余额解析失败")
        })?;

        if balance <= Decimal::ZERO {
            return Err(AppError::insufficient_balance(
                "余额不足，当前余额 ≤ 0".to_string(),
            ));
        }

        Ok(balance)
    }

    /// 计算最大并发数
    pub fn max_concurrent(&self, balance: &Decimal) -> i64 {
        if self.base_points <= 0 {
            return 1;
        }
        let max = (balance / Decimal::from(self.base_points)).floor();
        max.to_i64().unwrap_or(0).max(1)
    }

    /// 尝试获取并发许可
    pub fn try_acquire_concurrency(
        &self,
        token_str: &str,
        max_conc: i64,
    ) -> Result<ConcurrencyGuard, AppError> {
        let counter = self.concurrency
            .entry(token_str.to_string())
            .or_insert_with(|| Arc::new(AtomicI64::new(0)))
            .clone();

        let current = counter.fetch_add(1, Ordering::SeqCst);

        if current >= max_conc {
            counter.fetch_sub(1, Ordering::SeqCst);
            return Err(AppError::rate_limited(format!(
                "请求过于频繁，最大并发数: {}", max_conc
            )));
        }

        Ok(ConcurrencyGuard {
            counter,
        })
    }

    /// 计算本次 AI 调用的消耗积分
    ///
    /// # 计费公式
    ///
    /// 最终价格 = 渠道单价 × 全局倍率 × 100
    /// 消耗积分 = ceil( (final_price × tokens / 1000) 之和, 4位小数 )
    ///
    /// 即：
    /// ```text
    /// final_price_input  = price_input  × multiplier × 100
    /// final_price_cached = price_cached × multiplier × 100
    /// final_price_output = price_output × multiplier × 100
    ///
    /// input_cost  = final_price_input  × input_tokens  / 1000
    /// cached_cost = final_price_cached × cached_tokens / 1000
    /// output_cost = final_price_output × output_tokens / 1000
    ///
    /// total = input_cost + cached_cost + output_cost
    ///
    /// 结果 = ceil(total × 10^decimal_places) / 10^decimal_places
    /// ```
    ///
    /// # 公式说明
    ///
    /// - **渠道单价 (price_xxx)**: 在渠道配置中设置，表示该渠道每 1000 tokens
    ///   的基准价格（单位：元/千tokens）。
    ///   例如 OpenAI 的 gpt-4o 可能是 输入 $0.0025/1K tokens。
    ///
    /// - **全局倍率 (multiplier)**: 在计费设置页面配置的全局加成系数。
    ///   允许管理员对所有渠道统一加价，例如设置 1.2 表示上浮 20%。
    ///   这是业务层面的定价调节手段。
    ///
    /// - **× 100**: 将"元"转换为"积分"。1 元 = 100 积分（固定汇率）。
    ///   积分为系统内部计费单位，用户通过充值获得积分来消费。
    ///
    /// - **÷ 1000**: 因为渠道单价是按"每千 tokens"报价的，所以
    ///   需要除以 1000 得到每个 token 的实际费用。
    ///
    /// - **ceil 到 4 位小数**: 向上取整保证运营方不会因舍入亏损。
    ///   例如 0.00003 → 0.0001（积分最小单位为 0.0001）。
    ///
    /// # 参数
    ///
    /// - `channel`: 渠道配置（含输入/缓存/输出三种单价、三种倍率）
    /// - `input_tokens`: 本次请求实际消耗的输入 token 数（prompt_tokens）
    /// - `cached_tokens`: 本次请求命中缓存的 token 数
    /// - `output_tokens`: 本次请求生成的输出 token 数（completion_tokens）
    ///
    /// # 返回值
    ///
    /// 返回需要从用户余额中扣除的积分数量（Decimal 类型，精确到 4 位小数）。
    /// 最小返回值为 0（不会返回负数）。
    ///
    /// # 示例
    ///
    /// 假设渠道 gpt-4o 配置：
    /// - price_input  = 0.0025  (元/千tokens)
    /// - price_output = 0.01    (元/千tokens)
    /// - multiplier   = 1.2     (倍率 120%)
    ///
    /// 一次调用消耗 input=500, output=200 tokens：
    /// ```text
    /// final_price_input  = 0.0025 × 1.2 × 100 = 0.3
    /// final_price_output = 0.01   × 1.2 × 100 = 1.2
    ///
    /// input_cost  = 0.3 × 500 / 1000 = 0.15
    /// output_cost = 1.2 × 200 / 1000 = 0.24
    ///
    /// total = 0.15 + 0.24 = 0.39
    /// ceil(0.39) 到 4 位小数 = 0.3900
    /// ```
    /// 最终扣除 0.3900 积分。
    pub fn calculate_cost(
        &self,
        channel: &Channel,
        input_tokens: i64,
        cached_tokens: i64,
        output_tokens: i64,
    ) -> Result<CostDetail, AppError> {
        // ── 辅助闭包：将数据库中的 TEXT 金额解析为高精度 Decimal ──
        // Decimal 使用 10 进制内部表示，避免 f64 的浮点误差。
        let parse = |s: &str| -> Result<Decimal, AppError> {
            s.parse::<Decimal>().map_err(|_| {
                AppError::business(StatusCode::INTERNAL_SERVER_ERROR, "价格解析失败")
            })
        };

        // ── Step 1: 从渠道配置中读取三种场景的基准单价（元/100Wtokens）──
        let price_input  = parse(&channel.price_input)?;   // 输入单价
        let price_cached = parse(&channel.price_cached)?;  // 缓存命中单价
        let price_output = parse(&channel.price_output)?;  // 输出单价

        // ── Step 2: 读取全局计费倍率（管理员在计费设置中配置）──
        let multiplier = parse(&self.transaction_service.get_multiplier()?)?;

        // 固定汇率：1 元 = 100 积分
        let hundred = Decimal::from(100);

        // ── Step 3: 计算最终单价（元/100Wtokens 转换为 积分/100Wtokens）──
        // final_price = channel_price × multiplier × 100
        let final_price_input  = price_input  * multiplier * hundred;
        let final_price_cached = price_cached * multiplier * hundred;
        let final_price_output = price_output * multiplier * hundred;

        // ── Step 4: 根据实际 token 消耗计算各维度费用 ──
        // cost = final_price × tokens / 1000000
        let thousand   = Decimal::from(1000000);
        let input_cost  = final_price_input  * Decimal::from(input_tokens)  / thousand;
        let cached_cost = final_price_cached * Decimal::from(cached_tokens) / thousand;
        let output_cost = final_price_output * Decimal::from(output_tokens) / thousand;

        // ── Step 5: 汇总三个维度的费用 ──
        // total = input_cost + cached_cost + output_cost
        let total = input_cost + cached_cost + output_cost;

        // ── Step 6: 向上取整到指定位小数 ──
        // 公式: ceil(total × 10^dp) / 10^dp
        // 例如 dp=4, total=0.15003:
        //   scale = 10000
        //   scaled = 0.15003 × 10000 = 1500.3
        //   ceil(scaled) = 1501
        //   final = 1501 / 10000 = 0.1501
        let scale    = Decimal::from(10_i64.pow(self.decimal_places));
        let scaled   = total * scale;
        let ceil_val = scaled.ceil();            // 核心：余数直接进 1，不做四舍五入
        let final_cost = ceil_val / scale;

        // ── Step 7: 保证结果非负（极端情况下若上游返回负数，取 0）──
        Ok(CostDetail {
            total_cost: final_cost.max(Decimal::ZERO),
            input_price_final: final_price_input,
            cached_price_final: final_price_cached,
            output_price_final: final_price_output,
            multiplier,
        })
    }

}

/// 并发许可守卫：释放时自动 -1
pub struct ConcurrencyGuard {
    counter: Arc<AtomicI64>,
}

impl Drop for ConcurrencyGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}
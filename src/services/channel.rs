//! 渠道管理服务

use std::sync::Arc;
use stoolap::api::Database;

use crate::db::models::Channel;
use crate::error::AppError;
use crate::models::response::ChannelInfo;
use crate::snowflake::IdGenerator;

#[derive(Clone)]
pub struct ChannelService {
    db: Arc<Database>,
    id_gen: Arc<IdGenerator>,
}

impl ChannelService {
    pub fn new(db: Arc<Database>, id_gen: Arc<IdGenerator>) -> Self {
        Self { db, id_gen }
    }

    /// 查询所有渠道
    pub fn list(&self) -> Result<Vec<ChannelInfo>, AppError> {
        let channels: Vec<Channel> = self.db.query_as(
            "SELECT id, name, api_standard, price_input, price_cached, price_output, rate_input, rate_cached, rate_output, upstream_url, api_key, channel_type, price_reference_url, model, balance_url, created_at, updated_at FROM channels ORDER BY id DESC",
            (),
        )?;
        Ok(channels.into_iter().map(|c| channel_to_info(c)).collect())
    }

    /// 按名称查询单个渠道
    pub fn get_by_name(&self, name: &str) -> Result<Channel, AppError> {
        let rows: Vec<Channel> = self.db.query_as(
            "SELECT id, name, api_standard, price_input, price_cached, price_output, rate_input, rate_cached, rate_output, upstream_url, api_key, channel_type, price_reference_url, model, balance_url, created_at, updated_at FROM channels WHERE name = $1",
            (name.to_string(),),
        )?;
        rows.into_iter().next()
            .ok_or_else(|| AppError::not_found(format!("渠道不存在: {}", name)))
    }

    /// 按 id 查询
    pub fn get_by_id(&self, id: i64) -> Result<Channel, AppError> {
        let rows: Vec<Channel> = self.db.query_as(
            "SELECT id, name, api_standard, price_input, price_cached, price_output, rate_input, rate_cached, rate_output, upstream_url, api_key, channel_type, price_reference_url, model, balance_url, created_at, updated_at FROM channels WHERE id = $1",
            (id,),
        )?;
        rows.into_iter().next()
            .ok_or_else(|| AppError::not_found(format!("渠道不存在: id={}", id)))
    }

    /// 创建渠道（stoolap Params trait 最多支持 12 个参数，分两步写入）
    pub fn create(
        &self,
        name: &str,
        api_standard: &str,
        price_input: &str,
        price_cached: &str,
        price_output: &str,
        rate_input: &str,
        rate_cached: &str,
        rate_output: &str,
        upstream_url: &str,
        api_key: &str,
        channel_type: &str,
        price_reference_url: &str,
        model: &str,
        balance_url: &str,
    ) -> Result<ChannelInfo, AppError> {
        let new_id = self.id_gen.next_id();
        let now = crate::now_iso();

        // 第一步：插入核心字段 + 时间戳（共 12 个参数，达到 stoolap 上限）
        // 注意：created_at/updated_at 必须在 INSERT 中显式写入，
        // 因为 stoolap 不支持 strftime() 函数式 DEFAULT 表达式。
        let f = |s: &str| s.parse::<f64>().unwrap_or(0.0);
        self.db.execute(
            "INSERT INTO channels (id, name, api_standard, price_input, price_cached, price_output, \
             rate_input, rate_cached, rate_output, upstream_url, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            (
                new_id,
                name.to_string(),
                api_standard.to_string(),
                f(price_input),
                f(price_cached),
                f(price_output),
                f(rate_input),
                f(rate_cached),
                f(rate_output),
                upstream_url.to_string(),
                now.clone(),
                now,
            ),
        )?;

        // 第二步：补充 api_key、channel_type、price_reference_url、model、balance_url
        self.db.execute(
            "UPDATE channels SET api_key = $1, channel_type = $2, price_reference_url = $3, model = $4, balance_url = $5 WHERE id = $6",
            (api_key.to_string(), channel_type.to_string(), price_reference_url.to_string(), model.to_string(), balance_url.to_string(), new_id),
        )?;

        let ch = self.get_by_name(name)?;
        Ok(channel_to_info(ch))
    }

    /// 更新渠道（同样拆分为两步以避免超过 12 个参数限制）
    pub fn update(
        &self,
        id: i64,
        api_standard: Option<&str>,
        price_input: Option<&str>,
        price_cached: Option<&str>,
        price_output: Option<&str>,
        rate_input: Option<&str>,
        rate_cached: Option<&str>,
        rate_output: Option<&str>,
        upstream_url: Option<&str>,
        api_key: Option<&str>,
        channel_type: Option<&str>,
        price_reference_url: Option<&str>,
        model: Option<&str>,
        balance_url: Option<&str>,
    ) -> Result<ChannelInfo, AppError> {
        let existing = self.get_by_id(id)?;

        let new_api_std = api_standard.unwrap_or(&existing.api_standard);
        let new_pi = price_input.unwrap_or(&existing.price_input);
        let new_pc = price_cached.unwrap_or(&existing.price_cached);
        let new_po = price_output.unwrap_or(&existing.price_output);
        let new_ri = rate_input.unwrap_or(&existing.rate_input);
        let new_rc = rate_cached.unwrap_or(&existing.rate_cached);
        let new_ro = rate_output.unwrap_or(&existing.rate_output);
        let new_url = upstream_url.unwrap_or(&existing.upstream_url);
        let new_key = api_key.unwrap_or(&existing.api_key);
        let new_ct = channel_type.unwrap_or(&existing.channel_type);
        let new_pru = price_reference_url.unwrap_or(&existing.price_reference_url);
        let new_model = model.unwrap_or(&existing.model);
        let new_balance_url = balance_url.unwrap_or(&existing.balance_url);

        // 第一步：更新前 11 个字段 + updated_at（共 12 个参数）
        let f = |s: &str| s.parse::<f64>().unwrap_or(0.0);
        self.db.execute(
            "UPDATE channels SET api_standard=$1, price_input=$2, price_cached=$3, price_output=$4, \
             rate_input=$5, rate_cached=$6, rate_output=$7, upstream_url=$8, api_key=$9, \
             channel_type=$10, updated_at=$11 WHERE id=$12",
            (
                new_api_std.to_string(),
                f(new_pi),
                f(new_pc),
                f(new_po),
                f(new_ri),
                f(new_rc),
                f(new_ro),
                new_url.to_string(),
                new_key.to_string(),
                new_ct.to_string(),
                crate::now_iso(),
                existing.id,
            ),
        )?;

        // 第二步：更新 price_reference_url、model、balance_url
        self.db.execute(
            "UPDATE channels SET price_reference_url = $1, model = $2, balance_url = $3 WHERE id = $4",
            (new_pru.to_string(), new_model.to_string(), new_balance_url.to_string(), existing.id),
        )?;

        self.get_by_id(id).map(|c| channel_to_info(c))
    }

    /// 删除渠道
    pub fn delete(&self, id: i64) -> Result<(), AppError> {
        self.get_by_id(id)?;
        self.db.execute("DELETE FROM channels WHERE id = $1", (id,))?;
        Ok(())
    }
}

fn channel_to_info(c: Channel) -> ChannelInfo {
    ChannelInfo {
        id: c.id,
        name: c.name,
        api_standard: c.api_standard,
        price_input: c.price_input,
        price_cached: c.price_cached,
        price_output: c.price_output,
        rate_input: c.rate_input,
        rate_cached: c.rate_cached,
        rate_output: c.rate_output,
        upstream_url: c.upstream_url,
        api_key: c.api_key,
        channel_type: c.channel_type,
        price_reference_url: c.price_reference_url,
        model: c.model,
        balance_url: c.balance_url,
        created_at: c.created_at,
        updated_at: c.updated_at,
    }
}
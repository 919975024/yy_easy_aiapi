//! 数据层：基于 stoolap 嵌入式数据库
//!
//! stoolap 自带 MVCC、ACID 事务、多种索引类型，
//! 本模块负责：初始化 Database、建表、提供 FromRow 模型。

pub mod models;

use stoolap::api::Database;
use tracing::info;


/// 初始化数据库：打开（或创建）持久化数据库，执行迁移
pub fn init_database(db_path: &str) -> Result<Database, stoolap::Error> {
    info!("初始化 stoolap 数据库: {}", db_path);

    // 确保文件型数据库的父目录存在
    if let Some(file_path) = db_path
        .strip_prefix("file://")
        .or_else(|| db_path.strip_prefix("file:"))
    {
        // 去掉查询参数（?xxx）
        let clean_path = file_path.split('?').next().unwrap_or(file_path);
        if !clean_path.is_empty() {
            if let Some(parent) = std::path::Path::new(clean_path).parent() {
                std::fs::create_dir_all(parent).ok();
            }
        }
    }

    // 解析 temp_directory 参数，确保目录存在
    if let Some(q) = db_path.split('?').nth(1) {
        for pair in q.split('&') {
            if let Some(v) = pair.strip_prefix("temp_directory=") {
                let tmp_dir = v.strip_prefix("file://").unwrap_or(v);
                std::fs::create_dir_all(tmp_dir).ok();
                info!("temp_directory 已创建: {}", tmp_dir);
            }
        }
    }

    let db = Database::open(db_path)?;

    // 启用 WAL 模式 + 降低同步级别，大幅提升写入性能
    let _ = db.execute("PRAGMA journal_mode=WAL", ());
    let _ = db.execute("PRAGMA synchronous=NORMAL", ());

    create_tables(&db)?;
    info!("数据库初始化完成 (journal_mode=WAL, synchronous=NORMAL)");
    Ok(db)
}

fn create_tables(db: &Database) -> Result<(), stoolap::Error> {
    info!("开始建表...");

    create_channels_table(db)?;
    create_tokens_table(db)?;
    create_transactions_table(db)?;
    create_settings_table(db)?;

    info!("建表完成");
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// 建表
// ══════════════════════════════════════════════════════════════════════════════

fn create_channels_table(db: &Database) -> Result<(), stoolap::Error> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS channels (
            id                  INTEGER PRIMARY KEY,
            name                TEXT    NOT NULL,
            api_standard        TEXT    NOT NULL DEFAULT 'openai',
            price_input         REAL    NOT NULL DEFAULT 0,
            price_cached        REAL    NOT NULL DEFAULT 0,
            price_output        REAL    NOT NULL DEFAULT 0,
            rate_input          REAL    NOT NULL DEFAULT 1.0,
            rate_cached         REAL    NOT NULL DEFAULT 1.0,
            rate_output         REAL    NOT NULL DEFAULT 1.0,
            upstream_url        TEXT    NOT NULL DEFAULT '',
            api_key             TEXT    NOT NULL DEFAULT '',
            channel_type        TEXT    NOT NULL DEFAULT 'custom',
            price_reference_url TEXT    NOT NULL DEFAULT '',
            model               TEXT    NOT NULL DEFAULT '',
            balance_url         TEXT    NOT NULL DEFAULT '',
            created_at          TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', '+8 hours')),
            updated_at          TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', '+8 hours'))
        )",
        (),
    )?;

    db.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_channels_name     ON channels(name)",          ())?;
    db.execute("CREATE INDEX        IF NOT EXISTS idx_channels_standard ON channels(api_standard)", ())?;

    Ok(())
}

fn create_tokens_table(db: &Database) -> Result<(), stoolap::Error> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS tokens (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            token      TEXT    NOT NULL,
            label      TEXT    NOT NULL DEFAULT '',
            balance    REAL    NOT NULL DEFAULT 0,
            created_at TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', '+8 hours'))
        )",
        (),
    )?;

    db.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_tokens_token ON tokens(token)", ())?;
    db.execute("CREATE INDEX        IF NOT EXISTS idx_tokens_label ON tokens(label)", ())?;

    Ok(())
}

fn create_transactions_table(db: &Database) -> Result<(), stoolap::Error> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS transactions (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            token         TEXT    NOT NULL,
            token_prefix  TEXT    NOT NULL DEFAULT '',
            label         TEXT    NOT NULL DEFAULT '',
            change_type   TEXT    NOT NULL,
            amount        REAL    NOT NULL DEFAULT 0,
            balance_after REAL    NOT NULL DEFAULT 0,
            channel_name  TEXT    NOT NULL DEFAULT '',
            request_id    TEXT    NOT NULL DEFAULT '',
            operator      TEXT    NOT NULL DEFAULT 'api',
            metadata      TEXT    NOT NULL DEFAULT '{}',
            remark        TEXT    NOT NULL DEFAULT '',
            created_at    TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', '+8 hours')),
            input_tokens  INTEGER NOT NULL DEFAULT 0,
            input_price   REAL    NOT NULL DEFAULT 0,
            cached_tokens INTEGER NOT NULL DEFAULT 0,
            cached_price  REAL    NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            output_price  REAL    NOT NULL DEFAULT 0,
            multiplier    REAL    NOT NULL DEFAULT 1.0
        )",
        (),
    )?;

    db.execute("CREATE INDEX IF NOT EXISTS idx_trans_token_created        ON transactions(token,        created_at)", ())?;
    db.execute("CREATE INDEX IF NOT EXISTS idx_trans_label_created        ON transactions(label,        created_at)", ())?;
    db.execute("CREATE INDEX IF NOT EXISTS idx_trans_token_prefix_created ON transactions(token_prefix, created_at)", ())?;
    db.execute("CREATE INDEX IF NOT EXISTS idx_trans_created_at           ON transactions(created_at)",               ())?;

    Ok(())
}

fn create_settings_table(db: &Database) -> Result<(), stoolap::Error> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            id    INTEGER PRIMARY KEY,
            name  TEXT NOT NULL UNIQUE,
            value TEXT NOT NULL
        )",
        (),
    )?;

    let existing = db.query(
        "SELECT 1 FROM settings WHERE name = 'price_multiplier'",
        (),
    ).ok().and_then(|r| r.into_iter().next());
    if existing.is_none() {
        let _ = db.execute(
            "INSERT INTO settings (id, name, value) VALUES (1, 'price_multiplier', '1.0')",
            (),
        );
    }

    Ok(())
}

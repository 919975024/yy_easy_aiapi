//! 编译时嵌入 HTML 模板，无需外部 templates/ 目录
//!
//! 使用 `include_str!` 在编译阶段将模板内容写入二进制。

use tera::Tera;

/// 创建预加载所有模板的 Tera 实例（编译时嵌入）
pub fn create_tera() -> Result<Tera, tera::Error> {
    let mut tera = Tera::default();
    tera.add_raw_template("base.html", include_str!("../templates/base.html"))?;
    tera.add_raw_template("channels.html", include_str!("../templates/channels.html"))?;
    tera.add_raw_template("tokens.html", include_str!("../templates/tokens.html"))?;
    tera.add_raw_template("recharge.html", include_str!("../templates/recharge.html"))?;
    tera.add_raw_template("transactions.html", include_str!("../templates/transactions.html"))?;
    tera.add_raw_template("pricing.html", include_str!("../templates/pricing.html"))?;
    tera.add_raw_template("login.html", include_str!("../templates/login.html"))?;
    Ok(tera)
}

# yy_easy_aiapi

轻量级 AI 代理网关。兼容 OpenAI API 格式，提供 **Token 管理、计费扣费、渠道路由、流水审计、管理后台**，单实例支持 5000-8000 终端用户。


---

## 1. 项目简介

```
终端用户 ──→ yy_easy_aiapi ──→ 上游 AI 服务（OpenAI / DeepSeek / 自定义）
                │
                ├── Token 鉴权 & 余额扣费
                ├── 渠道路由 & 计费倍率
                ├── 并发控制
                └── 流水审计
```

**核心功能**：
- **AI 代理** — `POST /v1/chat/completions`，兼容 OpenAI SDK，支持流式/非流式
- **Token 管理** — 生成/删除/查询，格式 `{port}_{uuid}`
- **余额系统** — 充值/扣减/消费，计费公式 `渠道单价 × 全局倍率 × 100 × tokens / 1,000,000`，向上取整
- **渠道管理** — 多上游配置，按 model 字段动态路由
- **并发控制** — `max_concurrent = 余额 / base_points`，超限返回 429
- **管理后台** — Web 页面，Token/渠道/计费/流水可视化
- **Swagger** — `/swagger` 在线 API 文档

---

## 2. 安装部署

### 2.1 下载二进制包

| 平台 | 文件 |
|---|---|
| Windows | `yy_easy_aiapi.exe` |
| Linux x86_64 | `yy_easy_aiapi` |

### 2.2 部署目录结构

```
/opt/yy_easy_aiapi/          (或 C:\app\)
├── yy_easy_aiapi             # 二进制
├── config.toml               # 配置文件（修改后重启生效）
├── templates/                # 管理后台 HTML 模板
└── db/                       # 数据目录（自动创建）
```

### 2.3 启动

```bash
# Linux
chmod +x yy_easy_aiapi
./yy_easy_aiapi

# Windows
yy_easy_aiapi.exe
```

### 2.4 访问地址

| 地址 | 说明 |
|---|---|
| `http://{host}:{port}/yy_easy_aiapi/swagger` | API 文档（Swagger UI） |
| `http://{host}:{port}/yy_easy_aiapi/admin` | 管理后台（渠道管理） |
| `http://{host}:{port}/yy_easy_aiapi/admin/tokens` | Token 管理 |
| `http://{host}:{port}/yy_easy_aiapi/admin/recharge` | 充值/扣减 |
| `http://{host}:{port}/yy_easy_aiapi/admin/transactions` | 流水查询 |
| `http://{host}:{port}/yy_easy_aiapi/admin/pricing` | 计费设置 |

默认端口 7001，可在 `config.toml` 修改。

---

## 3. 编译打包

### 3.1 环境要求

- Rust 1.75+
- `config.toml` + `templates/` 目录

### 3.2 编译

```bash
# 开发编译
cargo build

# 生产编译（推荐）
cargo build --release
```

> Release profile 已预设 `lto = "thin"`, `codegen-units = 1`, `opt-level = 3`，二进制在 `target/release/`。

### 3.3 打包

```bash
# Linux
cp target/release/yy_easy_aiapi ./dist/
cp config.toml config.example.toml ./dist/
cp -r templates ./dist/
tar -czf yy_easy_aiapi-linux.tar.gz dist/

# Windows
copy target\release\yy_easy_aiapi.exe dist\
copy config.toml dist\
copy config.example.toml dist\
xcopy templates dist\templates\ /E
Compress-Archive -Path dist\* -DestinationPath yy_easy_aiapi-windows.zip
```

### 3.4 交叉编译

```bash
# Windows 交叉编译到 Linux
rustup target add x86_64-unknown-linux-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

---

## 4. 单实例用户建议

测试环境：release 编译，stoolap 数据库，AI Mock（10 tokens × 3 维度），100 并发。

| 场景 | QPS | avg | 说明 |
|---|---|---|---|
| AI 代理全链路 | **1,344** | 73ms | 校验→转发→计费→扣费 |
| Token 写入 | 9,059 | 11ms | 管理端批量生成 |
| 余额查询 | 334 | 290ms | 随数据量退化 |
| 余额变更 | 140 | 704ms | 同步 UPDATE 瓶颈 |
| 流水查询 | 3,043 | 32ms | 索引分页，无瓶颈 |

### 推荐用户量（按 10s/请求 模型）

| 数据量 | 推荐用户数 | 说明 |
|---|---|---|
| < 10 万 token | **5,000 - 8,000** | 安全水位 |
| 10 - 50 万 token | 2,000 - 5,000 | SELECT/UPDATE 退化 |
| > 50 万 token | < 1,000 | 考虑扩容或换库 |

> 关键约束：余额查询（S2）随数据量退化是决定性因素。

---

## 5. 扩展方案

### 5.1 换数据库

将 stoolap 替换为 PostgreSQL / MySQL，支持千万级 token。

```
# config.toml
[database]
url = "postgresql://user:pass@localhost/yy_easy_aiapi"
```

需修改 `src/db/mod.rs` 中的数据库初始化逻辑，替换 `stoolap::Database` 为对应连接池（如 `sqlx`）。

### 5.2 横向扩展（按 token 前缀 + Nginx 转发）

Token 格式 `{port}_{uuid}`，其中 `port` 前缀可用于分片路由。

```
                     ┌─→ yy_easy_aiapi (7001)
Nginx ──→ token 前缀路由 ──→ yy_easy_aiapi (7002)
                     └─→ yy_easy_aiapi (7003)
```

**Nginx 配置示例**：

```nginx
upstream backend_7001 {
    server 127.0.0.1:7001;
}
upstream backend_7002 {
    server 127.0.0.1:7002;
}

map $http_authorization $backend {
    "~^Bearer 7001_" backend_7001;
    "~^Bearer 7002_" backend_7002;
    default backend_7001;
}

server {
    listen 80;
    location /yy_easy_aiapi/ {
        proxy_pass http://$backend;
        proxy_set_header Host $host;
    }
}
```

每个实例独立部署，数据库不共享，token 按端口号前缀路由到对应实例。扩容只需启动新端口实例 + 更新 Nginx 路由规则。

---

## 6. API 对接

### 6.1 前置准备

1. 创建渠道（管理后台或 API）
2. 生成 Token（管理后台或 API）
3. 给 Token 充值

### 6.2 终端用户接入

用户拿到 Token 后，直接替换 OpenAI SDK 的 `base_url` 和 `api_key`：

```python
# OpenAI SDK 对接
from openai import OpenAI

client = OpenAI(
    base_url="http://{host}:{port}/yy_easy_aiapi/v1",
    api_key="7001_a1b2c3d4e5f6..."  # 你的 Token
)

response = client.chat.completions.create(
    model="gpt-4o",               # 对应渠道名
    messages=[{"role": "user", "content": "Hello"}]
)
```

```bash
# curl 对接
curl http://{host}:{port}/yy_easy_aiapi/v1/chat/completions \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o","messages":[{"role":"user","content":"hi"}]}'
```

### 6.3 管理 API

所有管理 API 需要 `Authorization: Bearer {api_token}`。

#### Token 管理

```bash
# 生成 Token
curl -X POST http://{host}:{port}/yy_easy_aiapi/api/token/generate \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{"label":"我的应用"}'

# 查询余额
curl "http://{host}:{port}/yy_easy_aiapi/api/token/balance?token={token}" \
  -H "Authorization: Bearer {api_token}"

# 充值
curl -X POST http://{host}:{port}/yy_easy_aiapi/api/token/recharge \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{"token":"{token}","amount":"1000"}'
```

#### 渠道管理

```bash
# 创建渠道
curl -X POST http://{host}:{port}/yy_easy_aiapi/api/channels \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{
    "name":"gpt-4o",
    "api_standard":"openai",
    "price_input":"0.01",
    "price_output":"0.03",
    "upstream_url":"https://api.openai.com",
    "api_key":"sk-your-openai-key"
  }'
```

#### 计费设置

```bash
# 设置全局倍率
curl -X POST http://{host}:{port}/yy_easy_aiapi/admin/pricing/api \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{"multiplier":"1.5"}'
```

### 6.4 API 参考

| 端点 | 方法 | 鉴权 | 说明 |
|---|---|---|---|
| `/v1/chat/completions` | POST | Token | AI 代理（兼容 OpenAI） |
| `/api/token/generate` | POST | api_token | 生成 Token |
| `/api/tokens` | GET | api_token | Token 列表（分页） |
| `/api/tokens/:token` | DELETE | api_token | 删除 Token |
| `/api/token/recharge` | POST | api_token | 充值 |
| `/api/token/deduct` | POST | api_token | 扣减 |
| `/api/token/balance` | GET | api_token | 查询余额 |
| `/api/token/transactions` | GET | api_token | 流水查询 |
| `/api/channels` | GET/POST | api_token | 渠道列表/创建 |
| `/api/channels/:id` | PUT/DELETE | api_token | 更新/删除渠道 |
| `/api/channels/:id/test` | POST | api_token | 测试渠道连通性 |
| `/admin/pricing/api` | GET/POST | api_token | 计费倍率查询/设置 |
| `/admin/transactions/api` | GET | api_token | 管理员流水（含计费详情） |

在线文档：`http://{host}:{port}/yy_easy_aiapi/swagger`
## 7 后台截图
渠道管理
@image imgs/channel.png
token管理
@image imgs/token.png
积分操作
@image imgs/addOrSub.png
消费日志
@image imgs/log.png
全部倍率
@image imgs/costX.png
## 8 联系作者
- issue 提交
- 邮箱 919975024@qq.com 
- 个人主页： www.yysoftqa.com
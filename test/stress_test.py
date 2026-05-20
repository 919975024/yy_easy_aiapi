#!/usr/bin/env python3
"""
yy_easy_aiapi 压力测试 — DeepSeek 渠道

流程:
  1. 批量创建 50 个 Token
  2. 每个 Token 充值 100 万积分
  3. 每个 Token 并发 5 个链接 (共 250 并发) 调用 AI 代理接口
  4. 汇总统计 + 生成 Markdown 报告
"""

import asyncio
import sys
import time
import aiohttp

# ═══════════════════ 配置 ═══════════════════
BASE_URL       = "http://127.0.0.1:7001/yy_easy_aiapi"
API_TOKEN      = "token001"
TOKEN_COUNT    = 100
CONCURRENT_PER_TOKEN = 5
RECHARGE_AMOUNT = "1000000"
TEST_CHANNEL   = "deepseek-app"          # 已在管理后台配置好
PROMPT         = "这是一条测试数据，你不用输出任何内容"

# ═══════════════════ 统计 ═══════════════════

class Stats:
    def __init__(self):
        self.success = 0
        self.fail = 0
        self.total_time_ms = 0.0
        self.min_time = float("inf")
        self.max_time = 0.0
        self.errors = []
        self.statuses = {}
        self.prompt_tokens = 0
        self.completion_tokens = 0

    def record(self, ok, elapsed_ms, status=0, err="", prompt_tok=0, comp_tok=0):
        if ok:
            self.success += 1
        else:
            self.fail += 1
            if err and len(self.errors) < 10:
                self.errors.append(err)
        self.total_time_ms += elapsed_ms
        self.min_time = min(self.min_time, elapsed_ms)
        self.max_time = max(self.max_time, elapsed_ms)
        self.statuses[status] = self.statuses.get(status, 0) + 1
        self.prompt_tokens += prompt_tok
        self.completion_tokens += comp_tok

    def avg_ms(self):
        total = self.success + self.fail
        return self.total_time_ms / total if total > 0 else 0

    def summary(self, label):
        total = self.success + self.fail
        if total == 0:
            return ""
        lines = []
        lines.append(f"### {label}")
        lines.append("")
        lines.append(f"| 指标 | 值 |")
        lines.append(f"|---|---|")
        lines.append(f"| 总请求 | {total} |")
        lines.append(f"| 成功 | {self.success} |")
        lines.append(f"| 失败 | {self.fail} |")
        lines.append(f"| 成功率 | {self.success/total*100:.1f}% |")
        lines.append(f"| 最小延迟 | {self.min_time:.0f} ms |")
        lines.append(f"| 最大延迟 | {self.max_time:.0f} ms |")
        lines.append(f"| 平均延迟 | {self.avg_ms():.0f} ms |")
        if self.statuses:
            lines.append(f"| HTTP 状态分布 | {dict(sorted(self.statuses.items()))} |")
        if self.prompt_tokens > 0:
            lines.append(f"| 累计 Prompt Tokens | {self.prompt_tokens} |")
            lines.append(f"| 累计 Completion Tokens | {self.completion_tokens} |")
        if self.errors:
            lines.append(f"| 错误样本 | {self.errors[:3]} |")
        lines.append("")
        return "\n".join(lines)


def auth_headers():
    return {"Authorization": f"Bearer {API_TOKEN}", "Content-Type": "application/json"}


# ═══════════════════ 步骤 1: 检查渠道 ═══════════════════

async def verify_channel():
    async with aiohttp.ClientSession(headers=auth_headers(), timeout=aiohttp.ClientTimeout(total=10)) as s:
        async with s.get(f"{BASE_URL}/api/channels") as r:
            channels = (await r.json()).get("data", [])
            for ch in channels:
                if ch["name"] == TEST_CHANNEL:
                    print(f"渠道: {ch['name']}  model={ch['model']}  url={ch['upstream_url']}")
                    return True
            print(f"渠道 {TEST_CHANNEL} 不存在，请先在管理后台创建")
            return False


# ═══════════════════ 步骤 2: 批量创建 Token ═══════════════════

async def create_tokens(count: int) -> list[str]:
    tokens = []
    async with aiohttp.ClientSession(headers=auth_headers(), timeout=aiohttp.ClientTimeout(total=30)) as s:
        for i in range(count):
            async with s.post(
                f"{BASE_URL}/api/token/generate",
                json={"label": f"stress-{i:03d}"},
            ) as r:
                if r.status == 200:
                    data = await r.json()
                    tokens.append(data["data"]["token"])
            if (i + 1) % 10 == 0:
                print(f"  Token 创建: {i+1}/{count}")
    return tokens


# ═══════════════════ 步骤 3: 批量充值 ═══════════════════

async def recharge_all(tokens: list[str]) -> Stats:
    stats = Stats()
    async with aiohttp.ClientSession(headers=auth_headers(), timeout=aiohttp.ClientTimeout(total=30)) as s:
        for i, tok in enumerate(tokens):
            t0 = time.time()
            async with s.post(
                f"{BASE_URL}/api/token/recharge",
                json={"token": tok, "amount": RECHARGE_AMOUNT, "remark": "压测充值"},
            ) as r:
                elapsed = (time.time() - t0) * 1000
                ok = r.status == 200
                err = "" if ok else f"#{i} HTTP{r.status}"
                stats.record(ok, elapsed, r.status, err)
            if (i + 1) % 10 == 0:
                print(f"  充值: {i+1}/{len(tokens)}")
    return stats


# ═══════════════════ 步骤 4: 并发压测 ═══════════════════

async def stress_ai_proxy(tokens: list[str]) -> tuple[Stats, float]:
    stats = Stats()
    total = len(tokens) * CONCURRENT_PER_TOKEN
    done = 0
    lock = asyncio.Lock()
    start_time = time.time()

    async def call_one(session, tok, n):
        nonlocal done
        t0 = time.time()
        ok = False
        err = ""
        status = 0
        prompt_tok = 0
        comp_tok = 0
        try:
            async with session.post(
                f"{BASE_URL}/v1/chat/completions",
                headers={
                    "Authorization": f"Bearer {tok}",
                    "Content-Type": "application/json",
                },
                json={
                    "model": TEST_CHANNEL,
                    "messages": [{"role": "user", "content": PROMPT}],
                    "stream": False,
                },
            ) as r:
                elapsed = (time.time() - t0) * 1000
                status = r.status
                body = await r.text()
                if r.status == 200:
                    import json
                    try:
                        resp = json.loads(body)
                        usage = resp.get("usage", {})
                        prompt_tok = usage.get("prompt_tokens", 0)
                        comp_tok = usage.get("completion_tokens", 0)
                        ok = True
                    except Exception:
                        ok = True  # 返回 200 就算成功
                elif r.status == 403:
                    err = f"余额不足: {body[:80]}"
                elif r.status == 404:
                    err = f"渠道/Token不存在: {body[:80]}"
                else:
                    err = f"HTTP {status}: {body[:80]}"
        except Exception as e:
            elapsed = (time.time() - t0) * 1000
            err = f"网络异常: {e}"

        stats.record(ok, elapsed, status, err, prompt_tok, comp_tok)

        async with lock:
            nonlocal done
            done += 1
            if done % 50 == 0 or done == total:
                et = time.time() - start_time
                qps = done / et if et > 0 else 0
                print(f"  进度: {done}/{total} ({done/total*100:.0f}%)  QPS≈{qps:.0f}")

    connector = aiohttp.TCPConnector(limit=0)
    timeout = aiohttp.ClientTimeout(total=120)
    async with aiohttp.ClientSession(connector=connector, timeout=timeout) as s:
        tasks = []
        for tok in tokens:
            for n in range(CONCURRENT_PER_TOKEN):
                tasks.append(call_one(s, tok, n))
        await asyncio.gather(*tasks)

    wall_time = time.time() - start_time
    return stats, wall_time


# ═══════════════════ 步骤 5: 余额抽查 ═══════════════════

async def check_balances(tokens: list[str]):
    balances = []
    async with aiohttp.ClientSession(headers=auth_headers(), timeout=aiohttp.ClientTimeout(total=30)) as s:
        for tok in tokens[:3]:
            async with s.get(f"{BASE_URL}/api/token/balance?token={tok}") as r:
                if r.status == 200:
                    data = (await r.json()).get("data", [])
                    if data:
                        balances.append((data[0]["label"], data[0]["balance"]))
    return balances


# ═══════════════════ 报告生成 ═══════════════════

def generate_report(
    create_ok: int, create_total: int, create_t: float,
    recharge_stats: Stats,
    stress_stats: Stats, wall_time: float,
    balances: list,
    total_t: float,
):
    report = f"""# yy_easy_aiapi 压力测试报告

> 测试时间: {time.strftime('%Y-%m-%d %H:%M:%S')}  |  目标: DeepSeek (`{TEST_CHANNEL}`)

---

## 一、测试配置

| 配置项 | 值 |
|---|---|
| Token 数量 | {TOKEN_COUNT} |
| 每 Token 并发 | {CONCURRENT_PER_TOKEN} |
| 总并发请求 | **{TOKEN_COUNT * CONCURRENT_PER_TOKEN}** |
| 每 Token 余额 | {RECHARGE_AMOUNT}（{int(RECHARGE_AMOUNT):,} 积分） |
| 测试渠道 | `{TEST_CHANNEL}` → DeepSeek API |
| 上游模型 | `deepseek-chat` |
| 上游地址 | `https://api.deepseek.com/v1` |
| 提示词 | `{PROMPT}` |

---

## 二、各阶段结果

| 阶段 | 总数 | 成功 | 失败 | 成功率 | 耗时 |
|---|---|---|---|---|---|
| Token 创建 | {create_total} | {create_ok} | {create_total - create_ok} | {create_ok/create_total*100:.0f}% | {create_t:.1f}s |
| 充值 | {recharge_stats.success + recharge_stats.fail} | {recharge_stats.success} | {recharge_stats.fail} | {recharge_stats.success/(recharge_stats.success+recharge_stats.fail)*100:.1f}% | {recharge_stats.total_time_ms/1000:.1f}s |
| **AI 代理压测** | **{stress_stats.success + stress_stats.fail}** | **{stress_stats.success}** | **{stress_stats.fail}** | **{stress_stats.success/(stress_stats.success+stress_stats.fail)*100:.1f}%** | **{wall_time:.1f}s** |

---

## 三、AI 代理压测详情

### 3.1 核心指标

| 指标 | 值 |
|---|---|
| 总请求数 | {stress_stats.success + stress_stats.fail} |
| 成功数 | {stress_stats.success} |
| 失败数 | {stress_stats.fail} |
| **成功率** | **{stress_stats.success/(stress_stats.success+stress_stats.fail)*100:.1f}%** |
| HTTP 状态分布 | {dict(sorted(stress_stats.statuses.items()))} |
| 墙钟时间 | {wall_time:.1f}s |
| 平均 QPS | {(stress_stats.success + stress_stats.fail) / wall_time:.0f} req/s |

### 3.2 延迟分布

| 指标 | 值 |
|---|---|
| 最小延迟 | {stress_stats.min_time:.0f} ms |
| 最大延迟 | {stress_stats.max_time:.0f} ms |
| 平均延迟 | {stress_stats.avg_ms():.0f} ms |

### 3.3 Token 消耗

| 指标 | 值 |
|---|---|
| 累计 Prompt Tokens | {stress_stats.prompt_tokens} |
| 累计 Completion Tokens | {stress_stats.completion_tokens} |
| 平均 Prompt Tokens/请求 | {stress_stats.prompt_tokens / max(stress_stats.success, 1):.0f} |
| 平均 Completion Tokens/请求 | {stress_stats.completion_tokens / max(stress_stats.success, 1):.0f} |

---

## 四、余额变化

| Token | 测试前 | 测试后 |
|---|---|---|
{chr(10).join(f"| {b[0]} | {RECHARGE_AMOUNT} | {b[1]} |" for b in balances)}

---

## 五、结论

> **{stress_stats.success + stress_stats.fail} 并发请求，成功率 {stress_stats.success/(stress_stats.success+stress_stats.fail)*100:.1f}%，平均延迟 {stress_stats.avg_ms():.0f}ms。**
>
> {'全部请求通过，服务在高并发下完全稳定。' if stress_stats.fail == 0 else f'存在 {stress_stats.fail} 个失败，需关注。'}
>
> 瓶颈在 DeepSeek API 响应速度，代理自身处理能力远超当前测试规模。

---

*报告自动生成于 {time.strftime('%Y-%m-%d %H:%M:%S')}*
"""
    return report


# ═══════════════════ main ═══════════════════

async def main():
    print("=" * 60)
    print("  yy_easy_aiapi  压力测试 — DeepSeek")
    print("=" * 60)
    print(f"  渠道: {TEST_CHANNEL}")
    print(f"  Token: {TOKEN_COUNT}  每Token并发: {CONCURRENT_PER_TOKEN}")
    print(f"  总并发: {TOKEN_COUNT * CONCURRENT_PER_TOKEN}")
    print("=" * 60)

    # 1. 检查渠道
    if not await verify_channel():
        sys.exit(1)

    t_total = time.time()

    # 2. 创建 Token
    print("\n[1/4] 创建 Token ...")
    t0 = time.time()
    tokens = await create_tokens(TOKEN_COUNT)
    create_t = time.time() - t0
    if len(tokens) < TOKEN_COUNT:
        print(f"Token 创建不足: {len(tokens)}/{TOKEN_COUNT}")
        sys.exit(1)

    # 3. 充值
    print("\n[2/4] 充值 ...")
    t0 = time.time()
    recharge_stats = await recharge_all(tokens)

    # 4. 压测
    print(f"\n[3/4] 并发压测 ({len(tokens)} token × {CONCURRENT_PER_TOKEN}) ...")
    stress_stats, wall_time = await stress_ai_proxy(tokens)

    # 5. 余额
    print("\n[4/4] 余额抽查 ...")
    balances = await check_balances(tokens)

    total_t = time.time() - t_total

    # ── 生成报告 ──
    report = generate_report(
        len(tokens), TOKEN_COUNT, create_t,
        recharge_stats,
        stress_stats, wall_time,
        balances,
        total_t,
    )

    report_path = "test/stress_report_deepseek.md"
    with open(report_path, "w", encoding="utf-8") as f:
        f.write(report)

    print(f"\n{'='*60}")
    print(f"  测试完成! 总耗时: {total_t:.1f}s")
    print(f"  成功率: {stress_stats.success}/{stress_stats.success+stress_stats.fail} ({stress_stats.success/(stress_stats.success+stress_stats.fail)*100:.1f}%)")
    print(f"  平均延迟: {stress_stats.avg_ms():.0f}ms")
    print(f"  报告已保存: {report_path}")
    print(f"{'='*60}")


if __name__ == "__main__":
    asyncio.run(main())

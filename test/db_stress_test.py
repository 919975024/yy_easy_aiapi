#!/usr/bin/env python3
"""
DB Stress Test — 使用 aiohttp 连接池，每个并发一个持久连接
"""
import asyncio, aiohttp, time, sys, os, random, json, statistics
from datetime import datetime, timedelta, timezone

BASE = "http://127.0.0.1:7001/yy_easy_aiapi"
API_TOKEN = "token001"

# ─── 配置 ───
S1_TOTAL = 50_000         # Token INSERT 总量
S1_CONC  = 100            # 写入并发

S2_TOTAL = 5_000          # Token SELECT 总量
S2_CONC  = 100            # 读取并发

S3_TOTAL = 50_000         # 流水 INSERT 总量
S3_CONC  = 100            # 写入并发

S4_TOTAL = 5_000          # 流水 SELECT 总量
S4_CONC  = 100            # 读取并发

# ─── 辅助: 读场景任务分配 (支持 total <= concurrent) ───
def scene_read_tasks(total, conc, worker_fn, session):
    if total <= conc:
        return [worker_fn(session, 1) for _ in range(total)]
    per = total // conc
    tasks = []
    for i in range(conc):
        cnt = per if i < conc - 1 else total - per * (conc - 1)
        tasks.append(worker_fn(session, cnt))
    return tasks

# ─── 统计 ───
class Stat:
    def __init__(self):
        self.ok = 0; self.fail = 0; self.lats = []; self.errs = []
        self._lock = asyncio.Lock()
    async def add(self, ok, lat, err=""):
        async with self._lock:
            if ok: self.ok += 1
            else: self.fail += 1; self.errs.append(err[:80])
            self.lats.append(lat)
    @property
    def total(self): return self.ok + self.fail
    @property
    def rate(self): return f"{self.ok/self.total*100:.1f}%" if self.total else "N/A"
    @property
    def avg(self): return statistics.mean(self.lats) if self.lats else 0
    @property
    def p50(self): return sorted(self.lats)[len(self.lats)//2] if self.lats else 0
    @property
    def min_lat(self): return min(self.lats) if self.lats else 0
    @property
    def max_lat(self): return max(self.lats) if self.lats else 0

# ─── 场景 1: Token INSERT ───
async def scene1_insert():
    stat = Stat()
    done = 0
    t0 = time.time()
    connector = aiohttp.TCPConnector(limit=S1_CONC, force_close=False)

    async def worker(session, start, end):
        nonlocal done
        for i in range(start, end):
            t1 = time.time()
            ok, err = False, ""
            try:
                async with session.post(f"{BASE}/api/token/generate",
                    json={"label": f"perf-{i:06d}"}) as r:
                    lat = (time.time() - t1) * 1000
                    ok = (r.status == 200)
                    if not ok: err = f"HTTP{r.status}"
            except Exception as e:
                lat = (time.time() - t1) * 1000; err = str(e)[:60]
            await stat.add(ok, lat, err)
            done += 1
            if done % 10000 == 0:
                et = time.time() - t0
                print(f"  [S1] {done}/{S1_TOTAL} ({done/S1_TOTAL*100:.0f}%)  QPS≈{done/et:.0f}")

    async with aiohttp.ClientSession(
        headers={"Authorization": f"Bearer {API_TOKEN}", "Content-Type": "application/json"},
        connector=connector, timeout=aiohttp.ClientTimeout(total=10)) as session:

        per = S1_TOTAL // S1_CONC
        tasks = [worker(session, i*per, (i+1)*per if i < S1_CONC-1 else S1_TOTAL) for i in range(S1_CONC)]
        print(f"[S1] Token INSERT — {S1_CONC}并发  {S1_TOTAL}条  (每worker {per}条)")
        await asyncio.gather(*tasks)

    wall = time.time() - t0
    print(f"  => ok={stat.ok} fail={stat.fail} ({stat.rate})  wall={wall:.1f}s  QPS={S1_TOTAL/wall:.0f}  "
          f"avg={stat.avg:.0f}ms p50={stat.p50:.0f}ms max={stat.max_lat:.0f}ms")
    return stat, wall

# ─── 场景 2: Token SELECT ───
async def scene2_query(tokens):
    stat = Stat()
    done = 0
    t0 = time.time()
    pool = tokens[:5000]
    connector = aiohttp.TCPConnector(limit=S2_CONC, force_close=False)

    async def worker(session, count):
        nonlocal done
        for _ in range(count):
            tok = random.choice(pool)
            t1 = time.time()
            ok, err = False, ""
            try:
                async with session.get(f"{BASE}/api/token/balance", params={"token": tok}) as r:
                    lat = (time.time() - t1) * 1000
                    ok = (r.status == 200)
                    if not ok: err = f"HTTP{r.status}"
            except Exception as e:
                lat = (time.time() - t1) * 1000; err = str(e)[:60]
            await stat.add(ok, lat, err)
            done += 1
            if done % 5000 == 0:
                et = time.time() - t0
                print(f"  [S2] {done}/{S2_TOTAL} ({done/S2_TOTAL*100:.0f}%)  QPS≈{done/et:.0f}")

    async with aiohttp.ClientSession(
        headers={"Authorization": f"Bearer {API_TOKEN}"},
        connector=connector, timeout=aiohttp.ClientTimeout(total=10)) as session:

        tasks = scene_read_tasks(S2_TOTAL, S2_CONC, worker, session)
        print(f"[S2] Token SELECT — {S2_CONC}并发  {S2_TOTAL}次 (按token查询)")
        await asyncio.gather(*tasks)

    wall = time.time() - t0
    print(f"  => ok={stat.ok} fail={stat.fail} ({stat.rate})  wall={wall:.1f}s  QPS={S2_TOTAL/wall:.0f}  "
          f"avg={stat.avg:.0f}ms p50={stat.p50:.0f}ms max={stat.max_lat:.0f}ms")
    return stat, wall

# ─── 场景 3: 流水 INSERT ───
async def scene3_insert_txn(tokens):
    stat = Stat()
    done = 0
    t0 = time.time()
    pool = tokens[:5000]
    connector = aiohttp.TCPConnector(limit=S3_CONC, force_close=False)

    async def worker(session, start, end):
        nonlocal done
        for i in range(start, end):
            tok = random.choice(pool)
            t1 = time.time()
            ok, err = False, ""
            try:
                async with session.post(f"{BASE}/api/token/recharge",
                    json={"token": tok, "amount": "10", "remark": f"perf-{i:06d}"}) as r:
                    lat = (time.time() - t1) * 1000
                    ok = (r.status == 200)
                    if not ok:
                        body = await r.text()
                        err = f"HTTP{r.status}:{body[:40]}"
            except Exception as e:
                lat = (time.time() - t1) * 1000; err = str(e)[:60]
            await stat.add(ok, lat, err)
            done += 1
            if done % 20000 == 0:
                et = time.time() - t0
                print(f"  [S3] {done}/{S3_TOTAL} ({done/S3_TOTAL*100:.0f}%)  QPS≈{done/et:.0f}")

    async with aiohttp.ClientSession(
        headers={"Authorization": f"Bearer {API_TOKEN}", "Content-Type": "application/json"},
        connector=connector, timeout=aiohttp.ClientTimeout(total=10)) as session:

        per = S3_TOTAL // S3_CONC
        tasks = [worker(session, i*per, (i+1)*per if i < S3_CONC-1 else S3_TOTAL) for i in range(S3_CONC)]
        print(f"[S3] Txn INSERT — {S3_CONC}并发  {S3_TOTAL}条  (每worker {per}条)")
        await asyncio.gather(*tasks)

    wall = time.time() - t0
    print(f"  => ok={stat.ok} fail={stat.fail} ({stat.rate})  wall={wall:.1f}s  QPS={S3_TOTAL/wall:.0f}  "
          f"avg={stat.avg:.0f}ms p50={stat.p50:.0f}ms max={stat.max_lat:.0f}ms")
    return stat, wall

# ─── 场景 4: 流水 SELECT ───
async def scene4_query_txn(tokens):
    stat = Stat()
    done = 0
    t0 = time.time()
    pool = tokens[:2000]
    base_date = datetime.now(timezone.utc)
    dates = [(base_date - timedelta(days=random.randint(0, 30))).strftime("%Y-%m-%d") for _ in range(100)]
    connector = aiohttp.TCPConnector(limit=S4_CONC, force_close=False)

    async def worker(session, count):
        nonlocal done
        for _ in range(count):
            tok = random.choice(pool)
            d = random.choice(dates)
            t1 = time.time()
            ok, err = False, ""
            try:
                async with session.get(f"{BASE}/api/token/transactions", params={
                    "token": tok, "start_time": f"{d}T00:00:00Z",
                    "end_time": f"{d}T23:59:59Z", "page": random.randint(1, 5), "page_size": 20
                }) as r:
                    lat = (time.time() - t1) * 1000
                    ok = (r.status == 200)
                    if not ok: err = f"HTTP{r.status}"
            except Exception as e:
                lat = (time.time() - t1) * 1000; err = str(e)[:60]
            await stat.add(ok, lat, err)
            done += 1
            if done % 5000 == 0:
                et = time.time() - t0
                print(f"  [S4] {done}/{S4_TOTAL} ({done/S4_TOTAL*100:.0f}%)  QPS≈{done/et:.0f}")

    async with aiohttp.ClientSession(
        headers={"Authorization": f"Bearer {API_TOKEN}"},
        connector=connector, timeout=aiohttp.ClientTimeout(total=15)) as session:

        tasks = scene_read_tasks(S4_TOTAL, S4_CONC, worker, session)
        print(f"[S4] Txn SELECT — {S4_CONC}并发  {S4_TOTAL}次 (按token+日期分页)")
        await asyncio.gather(*tasks)

    wall = time.time() - t0
    print(f"  => ok={stat.ok} fail={stat.fail} ({stat.rate})  wall={wall:.1f}s  QPS={S4_TOTAL/wall:.0f}  "
          f"avg={stat.avg:.0f}ms p50={stat.p50:.0f}ms max={stat.max_lat:.0f}ms")
    return stat, wall

# ─── main ───
async def main():
    print("=" * 60)
    print("  DB Stress Test (aiohttp connection pool)")
    print(f"  S1: Token INSERT  {S1_CONC}c x {S1_TOTAL}")
    print(f"  S2: Token SELECT  {S2_CONC}c x {S2_TOTAL}")
    print(f"  S3: Txn INSERT    {S3_CONC}c x {S3_TOTAL}")
    print(f"  S4: Txn SELECT    {S4_CONC}c x {S4_TOTAL}")
    print("=" * 60)

    t_total = time.time()

    # ── S1 ──
    print()
    s1, w1 = await scene1_insert()

    # 获取 token 样本（只取前几页，够查询采样用）
    print("\n[INFO] 获取 token 样本...")
    tokens = []
    connector = aiohttp.TCPConnector(limit=4)
    async with aiohttp.ClientSession(
        headers={"Authorization": f"Bearer {API_TOKEN}"},
        connector=connector, timeout=aiohttp.ClientTimeout(total=30)) as s:
        for page in [1, 2, 3]:
            async with s.get(f"{BASE}/api/tokens", params={"page": page, "page_size": 200}) as r:
                data = await r.json()
                items = data.get("data", {}).get("items", [])
                if not items: break
                for t in items:
                    tokens.append(t["token"])
    print(f"  获取到 {len(tokens)} 个 token（只用前几页，够查询采样用）")

    # ── S2 ──
    print()
    s2, w2 = await scene2_query(tokens)

    # ── S3 ──
    print()
    s3, w3 = await scene3_insert_txn(tokens)

    # ── S4 ── (等 WAL 稳定后再测)
    print("\n[INFO] 等待 WAL 稳定...")
    await asyncio.sleep(3)
    print()
    s4, w4 = await scene4_query_txn(tokens)

    total_wall = time.time() - t_total

    # ── 报告 ──
    db_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "db", "data.db")
    db_size = os.path.getsize(db_path) / 1024 / 1024 if os.path.exists(db_path) else 0
    now = time.strftime("%Y-%m-%d %H:%M:%S")

    def row(scene, conc, total, s, wall):
        return f"| {scene} | {conc} | {total} | {s.ok} | {s.fail} | {s.rate} | {wall:.1f}s | {total/wall:.0f} | {s.avg:.0f}ms | {s.p50:.0f}ms | {s.min_lat:.0f}ms | {s.max_lat:.0f}ms |"

    report = f"""# 数据库压力测试报告

> 测试时间: {now}  |  DB: {db_size:.1f} MB  |  profile: release (lto=thin, codegen-units=1, opt-level=3)

---

## 一、测试配置

| 场景 | 操作 | 并发 | 总量 | API |
|---|---|---|---|---|
| S1 | Token INSERT | {S1_CONC} | {S1_TOTAL} | POST /api/token/generate |
| S2 | Token SELECT | {S2_CONC} | {S2_TOTAL} | GET /api/token/balance |
| S3 | 流水 INSERT | {S3_CONC} | {S3_TOTAL} | POST /api/token/recharge |
| S4 | 流水 SELECT | {S4_CONC} | {S4_TOTAL} | GET /api/token/transactions |

---

## 二、测试结果

| 场景 | 并发 | 总量 | 成功 | 失败 | 成功率 | 耗时 | QPS | avg | p50 | min | max |
|---|---|---|---|---|---|---|---|---|---|---|---|
{row('S1 INSERT', S1_CONC, S1_TOTAL, s1, w1)}
{row('S2 SELECT', S2_CONC, S2_TOTAL, s2, w2)}
{row('S3 INSERT', S3_CONC, S3_TOTAL, s3, w3)}
{row('S4 SELECT', S4_CONC, S4_TOTAL, s4, w4)}

---

## 三、读写性能对比

| 操作类型 | QPS | avg | p50 | 并发 | 说明 |
|---|---|---|---|---|---|
| 写 S1 Token INSERT | {S1_TOTAL/w1:.0f} | {s1.avg:.0f}ms | {s1.p50:.0f}ms | {S1_CONC} | 单表 INSERT |
| 读 S2 Token SELECT | {S2_TOTAL/w2:.0f} | {s2.avg:.0f}ms | {s2.p50:.0f}ms | {S2_CONC} | 索引查余额 |
| 写 S3 流水 INSERT | {S3_TOTAL/w3:.0f} | {s3.avg:.0f}ms | {s3.p50:.0f}ms | {S3_CONC} | SELECT+UPDATE+INSERT |
| 读 S4 流水 SELECT | {S4_TOTAL/w4:.0f} | {s4.avg:.0f}ms | {s4.p50:.0f}ms | {S4_CONC} | 按 token+日期分页 |

---

## 四、总耗时

| 阶段 | 耗时 |
|---|---|
| S1 Token INSERT | {w1:.1f}s |
| S2 Token SELECT | {w2:.1f}s |
| S3 流水 INSERT | {w3:.1f}s |
| S4 流水 SELECT | {w4:.1f}s |
| **合计** | **{total_wall:.1f}s** |

---

*报告生成于 {now}*
"""
    rp = os.path.join(os.path.dirname(os.path.abspath(__file__)), "db_stress_report.md")
    with open(rp, "w", encoding="utf-8") as f:
        f.write(report)
    print(f"\n{'='*60}")
    print(f"  Total: {total_wall:.1f}s  Report: {rp}")
    print(f"{'='*60}")

if __name__ == "__main__":
    if sys.platform == "win32":
        asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())
    asyncio.run(main())

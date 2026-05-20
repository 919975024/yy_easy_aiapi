#!/usr/bin/env python3
"""充值/扣减 API 压测 — 200并发 10W条"""
import asyncio, aiohttp, time, sys, random, statistics

BASE = "http://127.0.0.1:7001/yy_easy_aiapi"
API_TOKEN = "token001"
HEADERS = {"Authorization": f"Bearer {API_TOKEN}", "Content-Type": "application/json"}

TOTAL = 100_000
CONC  = 200
POOL_SIZE = 5000   # token 池大小


class Stat:
    def __init__(self):
        self.ok = 0; self.fail = 0; self.lats = []; self.errs = []
        self._lock = asyncio.Lock()
    async def add(self, ok, lat, err=""):
        async with self._lock:
            if ok: self.ok += 1
            else: self.fail += 1; self.errs.append(err[:80])
            self.lats.append(lat)


async def gen_tokens(n):
    """生成 n 个 token"""
    tokens = []
    connector = aiohttp.TCPConnector(limit=50)
    async with aiohttp.ClientSession(headers=HEADERS, connector=connector,
                                      timeout=aiohttp.ClientTimeout(total=10)) as s:
        print(f"[PREP] 生成 {n} 个 token ...")
        t0 = time.time()
        sem = asyncio.Semaphore(50)
        async def one(i):
            async with sem:
                for retry in range(3):
                    try:
                        async with s.post(f"{BASE}/api/token/generate",
                                          json={"label": f"stress-{i:06d}"}) as r:
                            if r.status == 200:
                                data = await r.json()
                                return data["data"]["token"]
                    except:
                        await asyncio.sleep(0.1)
                return None
        tasks = [one(i) for i in range(n)]
        results = await asyncio.gather(*tasks)
        tokens = [t for t in results if t]
        print(f"  生成 {len(tokens)} 个 token, 耗时 {time.time()-t0:.1f}s")
    return tokens


async def stress_recharge(tokens):
    """200并发 10W条 充值"""
    stat = Stat()
    pool = tokens[:POOL_SIZE]
    done = 0
    t0 = time.time()
    connector = aiohttp.TCPConnector(limit=CONC, force_close=False)

    async def worker(session, start, end):
        nonlocal done
        for i in range(start, end):
            tok = random.choice(pool)
            t1 = time.time()
            ok, err = False, ""
            try:
                async with session.post(f"{BASE}/api/token/recharge",
                    json={"token": tok, "amount": "10", "remark": f"stress-{i:06d}"}) as r:
                    lat = (time.time() - t1) * 1000
                    ok = (r.status == 200)
                    if not ok:
                        body = await r.text()
                        err = f"HTTP{r.status}:{body[:60]}"
            except Exception as e:
                lat = (time.time() - t1) * 1000; err = str(e)[:60]
            await stat.add(ok, lat, err)
            done += 1
            if done % 20000 == 0:
                et = time.time() - t0
                print(f"  [{done}/{TOTAL} ({done/TOTAL*100:.0f}%)]  QPS≈{done/et:.0f}  avg={statistics.mean(stat.lats):.0f}ms")

    async with aiohttp.ClientSession(headers=HEADERS, connector=connector,
                                      timeout=aiohttp.ClientTimeout(total=15)) as session:
        per = TOTAL // CONC
        tasks = [worker(session, i*per, (i+1)*per if i < CONC-1 else TOTAL) for i in range(CONC)]
        print(f"\n[RUN] 充值压测 — {CONC}并发 x {TOTAL}条")
        await asyncio.gather(*tasks)

    wall = time.time() - t0
    lats = sorted(stat.lats)
    print(f"\n  {'='*50}")
    print(f"  成功: {stat.ok}  失败: {stat.fail}  ({stat.ok/stat.total*100:.1f}%)" if stat.total else "")
    print(f"  耗时: {wall:.1f}s  QPS: {TOTAL/wall:.0f}")
    print(f"  avg: {statistics.mean(lats):.0f}ms  p50: {lats[len(lats)//2]:.0f}ms  "
          f"p99: {lats[int(len(lats)*0.99)]:.0f}ms  max: {max(lats):.0f}ms")
    if stat.errs[:5]:
        print(f"  错误样本: {stat.errs[:5]}")
    return stat, wall


async def main():
    print("=" * 50)
    print(f"  充值 API 压测  {CONC}并发 x {TOTAL}条")
    print("=" * 50)

    # Step 1: 生成 token 池
    tokens = await gen_tokens(POOL_SIZE)
    if len(tokens) < 100:
        print("  FAIL: token 不足，退出")
        return

    # Step 2: 压测
    await stress_recharge(tokens)


if __name__ == "__main__":
    if sys.platform == "win32":
        asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())
    asyncio.run(main())

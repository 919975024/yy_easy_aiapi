//! Snowflake 雪花 ID 生成器
//!
//! 纯数字、全局唯一、趋势递增、无锁 CAS 实现。
//! 替代 stoolap 的 SELECT MAX(id)+1 方案，解决并发写入时的主键冲突。
//!
//! ID 结构 (51 bits, 约 2.25 千万亿):
//!   [41 bits 时间戳 (ms)] [10 bits 序列号 (0~1023)]
//!
//! 自定 Epoch: 2026-01-01 00:00:00 UTC
//! 可用至 2095 年，每毫秒最多 1024 个 ID。

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 自定 Epoch: 2026-01-01T00:00:00Z (ms)
const EPOCH_MS: i64 = 1767225600000;

/// 序列号占位 (10 bits = 1024 / ms)
const SEQUENCE_BITS: i64 = 10;
const MAX_SEQUENCE: i64 = (1 << SEQUENCE_BITS) - 1; // 1023

#[derive(Debug)]
pub struct IdGenerator {
    /// 高 41 bits: 时间戳偏移; 低 10 bits: 序列号
    state: AtomicI64,
}

impl IdGenerator {
    pub fn new() -> Self {
        let init_ts = current_ms() - EPOCH_MS;
        Self {
            state: AtomicI64::new(init_ts.max(0) << SEQUENCE_BITS),
        }
    }

    /// 生成下一个唯一 ID
    ///
    /// 无锁 CAS 循环：同一毫秒内递增序列号，序列号耗尽则自旋等待下一毫秒。
    pub fn next_id(&self) -> i64 {
        loop {
            let prev = self.state.load(Ordering::Acquire);
            let prev_ts = prev >> SEQUENCE_BITS;
            let prev_seq = prev & MAX_SEQUENCE;

            let now = (current_ms() - EPOCH_MS).max(0); // 防止系统时钟在 Epoch 之前

            let new_ts;
            let new_seq;

            if now == prev_ts {
                // 同一毫秒，序列号 +1
                if prev_seq >= MAX_SEQUENCE {
                    // 序列号耗尽（>1024/ms），自旋等下一毫秒
                    std::hint::spin_loop();
                    continue;
                }
                new_ts = now;
                new_seq = prev_seq + 1;
            } else if now > prev_ts {
                // 新的一毫秒，序列号归零
                new_ts = now;
                new_seq = 0;
            } else {
                // 时钟回拨：沿用旧时间戳，序列号 +1
                // 实际极少发生（NTP 微调），容忍度取决于序列号余量
                if prev_seq >= MAX_SEQUENCE {
                    std::hint::spin_loop();
                    continue;
                }
                new_ts = prev_ts;
                new_seq = prev_seq + 1;
            }

            let new_state = (new_ts << SEQUENCE_BITS) | new_seq;
            if self
                .state
                .compare_exchange_weak(prev, new_state, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return new_state;
            }
            // CAS 失败 → 另一线程抢先，重试
        }
    }
}

fn current_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[test]
    fn test_id_monotonic() {
        let gen = IdGenerator::new();
        let mut prev = gen.next_id();
        for _ in 0..100_000 {
            let curr = gen.next_id();
            assert!(curr > prev, "ID must increase: {} <= {}", curr, prev);
            prev = curr;
        }
    }

    #[test]
    fn test_id_unique_concurrent() {
        let gen = Arc::new(IdGenerator::new());
        let results = Arc::new(Mutex::new(Vec::new()));

        let threads: Vec<_> = (0..8)
            .map(|_| {
                let gen = gen.clone();
                let results = results.clone();
                std::thread::spawn(move || {
                    let mut local = Vec::with_capacity(10_000);
                    for _ in 0..10_000 {
                        local.push(gen.next_id());
                    }
                    results.lock().unwrap().extend(local);
                })
            })
            .collect();

        for t in threads {
            t.join().unwrap();
        }

        let all = results.lock().unwrap();
        let set: HashSet<i64> = all.iter().copied().collect();
        assert_eq!(all.len(), set.len(), "Found duplicate IDs");
        assert_eq!(all.len(), 80_000);
    }
}

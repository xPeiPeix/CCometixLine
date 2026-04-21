use crate::core::transcript::TranscriptStats;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 跨 ccline.exe 进程持久化的 transcript 解析 cache。
///
/// 每个 transcript 文件对应一份 cache，存在 `~/.claude/ccline/.transcript_cache_<hash>.json`。
/// 再次启动 ccline 时，若文件未被重写 / 截断，则只 parse 从 `file_size_at_parse` 之后追加的内容，
/// 并把增量 stats 累加到 `stats` 上，避免对长会话 transcript 做全量 O(N) 重解析。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptCache {
    /// 写 cache 时对应的 transcript 源文件绝对路径（校验用）。
    pub transcript_path: String,
    /// 上次 parse 结束时的文件大小 / 下次 seek 的 byte offset。
    pub file_size_at_parse: u64,
    /// 文件上次修改时间（RFC3339）。用于检测"文件被重写"。
    pub file_mtime: String,
    /// 累积至 `file_size_at_parse` 为止的统计。
    pub stats: TranscriptStats,
    /// 本 cache 写入时刻（RFC3339），调试用。
    pub cached_at: String,
}

/// 返回某 transcript 对应的 cache 文件路径：`~/.claude/ccline/.transcript_cache_<hash>.json`。
///
/// 使用 `DefaultHasher` 对 `transcript_path` 做 hash（非安全敏感场景，仅用于生成唯一文件名）。
///
/// **⚠️ 稳定性警告**：`std::collections::hash_map::DefaultHasher` 的算法是 std 实现细节，
/// 官方明确声明"可能随 Rust 版本变化"。升级 rustc 后同一 transcript path 会 hash 到不同
/// 文件名，旧 cache 变成孤儿文件残留在 `~/.claude/ccline/` 目录。
///
/// 后果较轻：升级 rustc 后第一次 ccline 启动退化为一次全量 parse（~几百毫秒），旧 cache
/// 每份几百字节的磁盘占用。若主人发现 `~/.claude/ccline/.transcript_cache_*.json` 堆积过多，
/// 可手动 `rm ~/.claude/ccline/.transcript_cache_*.json` 清理，下次 ccline 启动会自动重建。
pub fn cache_path(transcript_path: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let mut hasher = DefaultHasher::new();
    transcript_path.to_string_lossy().hash(&mut hasher);
    let hash = hasher.finish();
    Some(
        home.join(".claude")
            .join("ccline")
            .join(format!(".transcript_cache_{:016x}.json", hash)),
    )
}

/// 读取并反序列化 cache 文件。文件不存在 / JSON 非法 / 任意 IO 错误一律返回 None。
pub fn load_cache(transcript_path: &Path) -> Option<TranscriptCache> {
    let path = cache_path(transcript_path)?;
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 原子写 cache：先写 `.tmp` 再 `fs::rename` 覆盖旧文件，避免 ccline 正好读到写到一半的文件。
///
/// 父目录 `~/.claude/ccline/` 不存在时会被自动创建。`fs::rename` 在 Windows 下偶尔会因
/// target 被杀软扫描占用而失败——此时显式删除 `.tmp` 避免残渣堆积。
pub fn save_cache(cache: &TranscriptCache) -> io::Result<()> {
    let src_path = PathBuf::from(&cache.transcript_path);
    let target = cache_path(&src_path).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "home dir not found; cannot build cache_path",
        )
    })?;

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = target.with_extension("json.tmp");
    let json = serde_json::to_string(cache)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, json)?;
    if let Err(e) = fs::rename(&tmp, &target) {
        // rename 失败 → 清理 .tmp 避免残渣堆积，然后把原错误向上传递
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn mtime_rfc3339(t: SystemTime) -> String {
    DateTime::<Utc>::from(t).to_rfc3339()
}

/// 带 cache 的 parse 入口——自动选择全量或增量，处理文件切换 / 截断 / cache 损坏降级。
///
/// 决策树（注意：与 TODO 写的"mtime 变就全量"有差异，原因见下）：
/// - cache 不存在 / 路径不匹配                      → 全量 parse + 覆盖 cache
/// - cache 存在，但 `file_size_at_parse > file_size`（文件被截断 / 重写） → 全量 parse + 覆盖 cache
/// - cache 存在，`file_size_at_parse <= file_size`   → 增量 parse（size 相等时读 0 行，等同于直接返回 cache.stats）
///
/// **为什么不校验 mtime**：append 也会改 mtime，如果把"mtime 变"当作全量信号，则**所有正常会话**
/// （一直在 append 的 transcript）每次 ccline 启动都会触发全量 parse，增量 cache 永远不起作用，
/// Phase 2 的性能目标（~50ms）实现不了。`file_size_at_parse > file_size` 才是"截断/重写"的强信号
/// ——JSONL 只会追加不会原地改行，正常使用 size 单调递增。
///
/// **降级链**：增量失败 → 全量 parse 再试；全量也失败且有旧 cache → 返回旧 cache.stats
/// （显示略旧数据比所有 segment 变 `-` 体验更好）；所有路径都失败 → None。
///
/// `file_size == 0`（空 transcript）时跳过 save_cache——避免"每次启动都多一次无效磁盘写"的负优化。
pub fn parse_with_cache(transcript_path: &Path) -> Option<TranscriptStats> {
    let metadata = fs::metadata(transcript_path).ok()?;
    let file_size = metadata.len();
    let file_mtime = metadata
        .modified()
        .ok()
        .map(mtime_rfc3339)
        .unwrap_or_default();

    let existing = load_cache(transcript_path);

    let reusable_cache = existing.as_ref().filter(|c| {
        c.transcript_path == transcript_path.to_string_lossy()
            && c.file_size_at_parse <= file_size
    });

    let (stats, new_offset) = if let Some(cache) = reusable_cache {
        match TranscriptStats::parse_incremental(
            transcript_path,
            cache.file_size_at_parse,
            cache.stats.clone(),
        ) {
            Some(v) => v,
            None => {
                // 增量失败（并发截断等）→ 全量重试；全量也失败 → 保留旧 cache.stats 显示，
                // 避免 5 个 transcript-based segment 同时变 "-" 的糟糕体验
                TranscriptStats::parse_incremental(
                    transcript_path,
                    0,
                    TranscriptStats::default(),
                )
                .unwrap_or_else(|| (cache.stats.clone(), cache.file_size_at_parse))
            }
        }
    } else {
        // cache 缺失 / 不匹配 / 被截断：全量重 parse
        TranscriptStats::parse_incremental(transcript_path, 0, TranscriptStats::default())?
    };

    // 写回 cache；写失败不影响本次结果。空文件跳过写入避免无效 IO
    if file_size > 0 {
        let fresh = TranscriptCache {
            transcript_path: transcript_path.to_string_lossy().into_owned(),
            file_size_at_parse: new_offset,
            file_mtime,
            stats: stats.clone(),
            cached_at: Utc::now().to_rfc3339(),
        };
        let _ = save_cache(&fresh);
    }

    Some(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "{}_{}_{}.jsonl",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    fn write_lines(path: &Path, lines: &[&str]) {
        let mut f = fs::File::create(path).expect("create transcript");
        for l in lines {
            writeln!(f, "{}", l).unwrap();
        }
    }

    fn append_lines(path: &Path, lines: &[&str]) {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(path)
            .expect("append transcript");
        for l in lines {
            writeln!(f, "{}", l).unwrap();
        }
    }

    const USER_LINE: &str = r#"{"type":"user","message":{"content":[]}}"#;
    const ASSISTANT_LINE_A: &str = r#"{"type":"assistant","message":{"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5}}}"#;
    const ASSISTANT_LINE_B: &str = r#"{"type":"assistant","message":{"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":2}}}"#;

    #[test]
    fn full_parse_equals_two_step_incremental_parse() {
        // 目标：全量读 5 行 == 先读 3 行得 offset，再从 offset 增量读 2 行
        let path = unique_tmp_path("ccline_cache_incr_eq");

        // 第一批 3 行
        write_lines(&path, &[USER_LINE, ASSISTANT_LINE_A, USER_LINE]);

        let (step1_stats, offset1) = TranscriptStats::parse_incremental(
            &path,
            0,
            TranscriptStats::default(),
        )
        .expect("step1 parse ok");

        // 第二批 2 行 append
        append_lines(&path, &[ASSISTANT_LINE_B, USER_LINE]);

        let (incremental_final, _) =
            TranscriptStats::parse_incremental(&path, offset1, step1_stats.clone())
                .expect("step2 parse ok");

        // 对比：从 0 开始一次性全量 parse
        let full = TranscriptStats::parse(&path).expect("full parse ok");

        assert_eq!(
            incremental_final.input_tokens, full.input_tokens,
            "input_tokens mismatch"
        );
        assert_eq!(
            incremental_final.output_tokens, full.output_tokens,
            "output_tokens mismatch"
        );
        assert_eq!(
            incremental_final.turn_count, full.turn_count,
            "turn_count mismatch"
        );
        // Sanity: 3 user lines → turn_count = 3; 2 assistants stop_reason → last reason set
        assert_eq!(full.turn_count, 3);
        assert_eq!(full.input_tokens, 13);
        assert_eq!(full.output_tokens, 7);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parse_incremental_returns_none_when_file_truncated() {
        // 写 5 行 → 拿到 offset_full；然后截断到 2 行（size 变小）→ parse_incremental(from=offset_full) 必须返回 None
        let path = unique_tmp_path("ccline_cache_truncate");
        write_lines(
            &path,
            &[
                USER_LINE,
                ASSISTANT_LINE_A,
                USER_LINE,
                ASSISTANT_LINE_B,
                USER_LINE,
            ],
        );

        let (_stats5, offset5) =
            TranscriptStats::parse_incremental(&path, 0, TranscriptStats::default())
                .expect("initial parse ok");

        // 截断覆盖：只留 2 行
        write_lines(&path, &[USER_LINE, ASSISTANT_LINE_B]);

        let truncated = TranscriptStats::parse_incremental(
            &path,
            offset5,
            TranscriptStats::default(),
        );

        assert!(
            truncated.is_none(),
            "parse_incremental must return None when from_offset > new file_size (truncated)"
        );

        // 从 0 重新 parse 仍然成功（降级路径）
        let recovered = TranscriptStats::parse(&path).expect("full re-parse after truncate");
        assert_eq!(recovered.turn_count, 1); // 1 user line 剩下

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parse_incremental_does_not_count_half_line_bytes() {
        // 关键回归测试：半行 JSONL 末尾缺 \n 时，offset 不应被推进，下次补全后能完整读到
        let path = unique_tmp_path("ccline_cache_halfline");

        // 先写 2 行完整内容
        write_lines(&path, &[USER_LINE, ASSISTANT_LINE_A]);

        // 手动 append 半行（不带 \n）模拟 Claude Code 正在刷盘的中间态
        {
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("append half line");
            write!(f, r#"{{"type":"user","message":"#).expect("write partial");
        }

        let (stats_partial, offset_partial) =
            TranscriptStats::parse_incremental(&path, 0, TranscriptStats::default())
                .expect("parse ok");
        assert_eq!(stats_partial.turn_count, 1, "仅 1 完整 user line 被计入");
        assert_eq!(stats_partial.input_tokens, 10);

        // offset 不应包含半行的字节——文件大小大于 offset
        let file_size = fs::metadata(&path).unwrap().len();
        assert!(
            offset_partial < file_size,
            "offset ({}) 必须小于 file_size ({})，否则半行会被跳过",
            offset_partial, file_size
        );

        // 补全那一行为完整 user line
        {
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("complete half line");
            writeln!(f, r#"{{"content":[]}}}}"#).expect("write rest");
        }

        // 拼起来的完整行 = `{"type":"user","message":{"content":[]}}` → 合法 user line
        let (stats_after, _) =
            TranscriptStats::parse_incremental(&path, offset_partial, stats_partial.clone())
                .expect("parse after completion");
        assert_eq!(
            stats_after.turn_count, 2,
            "补全后的 user line 必须被计入（turn 1→2）"
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_cache_returns_none_for_corrupt_json() {
        // 手写一个非法 JSON 到 cache_path；load_cache 必须返回 None 不 panic
        // 为避免污染真实 home dir，用假的 transcript_path + save_cache 写入，然后 corrupt 覆盖
        if dirs::home_dir().is_none() {
            eprintln!("skip load_cache_returns_none_for_corrupt_json: no home dir");
            return;
        }

        let transcript_path = unique_tmp_path("ccline_cache_corrupt");
        write_lines(&transcript_path, &[USER_LINE, ASSISTANT_LINE_A]);

        // 构造一份合法 cache 先保存（确认 save/load 基本往返 OK）
        let stats = TranscriptStats::parse(&transcript_path).expect("parse ok");
        let legit = TranscriptCache {
            transcript_path: transcript_path.to_string_lossy().into_owned(),
            file_size_at_parse: fs::metadata(&transcript_path).unwrap().len(),
            file_mtime: String::new(),
            stats,
            cached_at: "2026-04-21T00:00:00Z".to_string(),
        };
        save_cache(&legit).expect("save ok");

        // 现在把该 cache 文件覆盖成非法 JSON
        let cpath = cache_path(&transcript_path).expect("cache_path ok");
        fs::write(&cpath, b"{not-valid-json").expect("write corrupt ok");

        let loaded = load_cache(&transcript_path);
        assert!(
            loaded.is_none(),
            "load_cache must return None for corrupt JSON without panicking"
        );

        // 清理 cache 文件和 transcript
        let _ = fs::remove_file(&cpath);
        let _ = fs::remove_file(&transcript_path);
    }

    #[test]
    fn parse_with_cache_uses_incremental_after_append() {
        // 集成测试：parse_with_cache 整条决策树——首次建 cache，append 后第二次走增量路径
        if dirs::home_dir().is_none() {
            eprintln!("skip parse_with_cache_uses_incremental_after_append: no home dir");
            return;
        }

        let path = unique_tmp_path("ccline_pwc_append");
        write_lines(&path, &[USER_LINE, ASSISTANT_LINE_A, USER_LINE]);

        // 首次：建立 cache
        let first = parse_with_cache(&path).expect("first call ok");
        assert_eq!(first.turn_count, 2);
        assert_eq!(first.input_tokens, 10);
        assert_eq!(first.output_tokens, 5);

        // cache 文件应该存在
        let cpath = cache_path(&path).expect("cache_path ok");
        assert!(cpath.exists(), "首次调用后 cache 文件必须存在");

        // append 2 行
        append_lines(&path, &[ASSISTANT_LINE_B, USER_LINE]);

        // 第二次：走增量路径，stats 累加
        let second = parse_with_cache(&path).expect("second call ok");

        // 跟从 0 全量 parse 的结果对比
        let full = TranscriptStats::parse(&path).expect("full ok");
        assert_eq!(second.turn_count, full.turn_count);
        assert_eq!(second.input_tokens, full.input_tokens);
        assert_eq!(second.output_tokens, full.output_tokens);
        assert_eq!(full.turn_count, 3);
        assert_eq!(full.input_tokens, 13);
        assert_eq!(full.output_tokens, 7);

        // 清理
        let _ = fs::remove_file(&cpath);
        let _ = fs::remove_file(&path);
    }
}

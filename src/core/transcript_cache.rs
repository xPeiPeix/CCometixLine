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
/// 父目录 `~/.claude/ccline/` 不存在时会被自动创建。
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
    fs::rename(&tmp, &target)?;
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
/// 任何 IO 失败都会 fallback 到全量 `parse_incremental`，并且不写 cache，保证 statusline 永远能返回。
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
        // 增量：基于 cache 的 stats 继续累加；若 parse_incremental 返回 None（如并发截断）降级全量
        TranscriptStats::parse_incremental(
            transcript_path,
            cache.file_size_at_parse,
            cache.stats.clone(),
        )
        .or_else(|| {
            TranscriptStats::parse_incremental(
                transcript_path,
                0,
                TranscriptStats::default(),
            )
        })?
    } else {
        // cache 缺失 / 不匹配 / 被截断：全量重 parse
        TranscriptStats::parse_incremental(transcript_path, 0, TranscriptStats::default())?
    };

    // 写回 cache；写失败不影响本次结果
    let fresh = TranscriptCache {
        transcript_path: transcript_path.to_string_lossy().into_owned(),
        file_size_at_parse: new_offset,
        file_mtime,
        stats: stats.clone(),
        cached_at: Utc::now().to_rfc3339(),
    };
    let _ = save_cache(&fresh);

    Some(stats)
}

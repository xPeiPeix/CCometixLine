use crate::core::transcript::TranscriptStats;
use serde::{Deserialize, Serialize};

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

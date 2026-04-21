use crate::config::TranscriptEntry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranscriptStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub turn_count: u32,
    pub tool_call_count: u32,
    pub tool_result_count: u32,
    pub tool_error_count: u32,
    pub last_stop_reason: Option<String>,
}

impl TranscriptStats {
    /// 全量 parse 入口：从头读完整个 transcript 文件。
    /// 文件不存在 / IO 失败返回 None。
    pub fn parse<P: AsRef<Path>>(transcript_path: P) -> Option<Self> {
        let (stats, _new_offset) =
            Self::parse_incremental(transcript_path, 0, TranscriptStats::default())?;
        Some(stats)
    }

    /// 增量 parse 入口：从 `from_offset` 字节处开始读，把新增行累加到 `init` 上。
    ///
    /// 返回 `(累积 stats, 新的 file_size_offset)`，调用方把 offset 回写到 cache 即可下次继续。
    /// 文件不存在 / IO 失败 / `from_offset > file_size`（说明文件被截断）返回 None，调用方
    /// 负责降级到全量 parse。
    ///
    /// **半行 tolerant**：若 ccline 启动时 Claude Code 恰好只把半行字节刷入磁盘（文件末尾
    /// 没有 `\n`），此函数**不会把该半行的字节计入返回的 offset**——下次 parse 将从上一条
    /// 完整行的末尾继续，残半行被 append 补全后会被完整解析，不会遗漏 usage。
    pub fn parse_incremental<P: AsRef<Path>>(
        transcript_path: P,
        from_offset: u64,
        init: TranscriptStats,
    ) -> Option<(Self, u64)> {
        let path = transcript_path.as_ref();
        if !path.exists() {
            return None;
        }

        let mut file = fs::File::open(path).ok()?;
        let file_size = file.metadata().ok()?.len();

        // 文件被截断：调用方应全量重 parse，而不是静默继续
        if from_offset > file_size {
            return None;
        }

        // 没新内容时快速返回，避免无意义 IO
        if from_offset == file_size {
            return Some((init, file_size));
        }

        if from_offset > 0 {
            file.seek(SeekFrom::Start(from_offset)).ok()?;
        }

        let mut reader = BufReader::new(file);
        let mut stats = init;
        // consumed_offset 只累加"读到的完整行"的字节数（含末尾 `\n`）；读到残半行时停下
        // 不累加，下次从同一 offset 重新读 → 残半行被补全后能被完整解析。
        let mut consumed_offset = from_offset;
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if !buf.ends_with('\n') {
                        // 残半行：本次刷盘未完整，丢弃 buf 不累加 offset
                        break;
                    }
                    consumed_offset += n as u64;
                    let trimmed = buf.trim_end_matches(|c| c == '\n' || c == '\r');
                    if !trimmed.is_empty() {
                        apply_line_to_stats(trimmed, &mut stats);
                    }
                }
                Err(_) => break,
            }
        }

        Some((stats, consumed_offset))
    }

    pub fn cache_hit_rate(&self) -> Option<f64> {
        let total = self.input_tokens + self.cache_read_tokens + self.cache_creation_tokens;
        if total == 0 {
            return None;
        }
        Some(self.cache_read_tokens as f64 / total as f64 * 100.0)
    }

    pub fn tool_success_rate(&self) -> Option<f64> {
        if self.tool_result_count == 0 {
            return None;
        }
        let success = self.tool_result_count.saturating_sub(self.tool_error_count);
        Some(success as f64 / self.tool_result_count as f64 * 100.0)
    }
}

fn apply_line_to_stats(line: &str, stats: &mut TranscriptStats) {
    let entry: TranscriptEntry = match serde_json::from_str(line) {
        Ok(e) => e,
        Err(_) => return,
    };

    match entry.r#type.as_deref() {
        Some("user") => {
            stats.turn_count += 1;
            if let Some(message) = &entry.message {
                if let Some(content) = &message.content {
                    count_tool_results(
                        content,
                        &mut stats.tool_result_count,
                        &mut stats.tool_error_count,
                    );
                }
            }
        }
        Some("assistant") => {
            if let Some(message) = &entry.message {
                if let Some(reason) = &message.stop_reason {
                    stats.last_stop_reason = Some(reason.clone());

                    if let Some(usage) = &message.usage {
                        let norm = usage.clone().normalize();
                        stats.input_tokens += norm.input_tokens as u64;
                        stats.output_tokens += norm.output_tokens as u64;
                        stats.cache_read_tokens += norm.cache_read_input_tokens as u64;
                        stats.cache_creation_tokens += norm.cache_creation_input_tokens as u64;
                    }
                }

                if let Some(content) = &message.content {
                    stats.tool_call_count += count_tool_uses(content);
                }
            }
        }
        _ => {}
    }
}

fn count_tool_results(content: &serde_json::Value, total: &mut u32, errors: &mut u32) {
    if let Some(arr) = content.as_array() {
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                *total += 1;
                if item.get("is_error").and_then(|e| e.as_bool()) == Some(true) {
                    *errors += 1;
                }
            }
        }
    }
}

fn count_tool_uses(content: &serde_json::Value) -> u32 {
    content
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                .count() as u32
        })
        .unwrap_or(0)
}

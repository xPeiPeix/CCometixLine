use crate::config::TranscriptEntry;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Default)]
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
    pub fn parse<P: AsRef<Path>>(transcript_path: P) -> Option<Self> {
        let path = transcript_path.as_ref();
        if !path.exists() {
            return None;
        }

        let file = fs::File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut stats = TranscriptStats::default();

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }

            let entry: TranscriptEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
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
                                stats.cache_read_tokens +=
                                    norm.cache_read_input_tokens as u64;
                                stats.cache_creation_tokens +=
                                    norm.cache_creation_input_tokens as u64;
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

        Some(stats)
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

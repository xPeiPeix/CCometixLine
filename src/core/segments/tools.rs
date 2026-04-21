use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct ToolsSegment;

impl ToolsSegment {
    pub fn new() -> Self {
        Self
    }
}

impl Segment for ToolsSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let stats = input.transcript_stats()?;
        if stats.tool_call_count == 0 {
            return None;
        }

        let primary = match stats.tool_success_rate() {
            Some(rate) => format!("{} ({:.0}%)", stats.tool_call_count, rate),
            None => stats.tool_call_count.to_string(),
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "tool_call_count".to_string(),
            stats.tool_call_count.to_string(),
        );
        metadata.insert(
            "tool_result_count".to_string(),
            stats.tool_result_count.to_string(),
        );
        metadata.insert(
            "tool_error_count".to_string(),
            stats.tool_error_count.to_string(),
        );
        if let Some(rate) = stats.tool_success_rate() {
            metadata.insert("success_rate".to_string(), format!("{:.2}", rate));
        }

        Some(SegmentData {
            primary,
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Tools
    }
}

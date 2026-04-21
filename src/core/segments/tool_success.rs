use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct ToolSuccessSegment;

impl ToolSuccessSegment {
    pub fn new() -> Self {
        Self
    }
}

impl Segment for ToolSuccessSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let stats = input.transcript_stats()?;
        let rate = stats.tool_success_rate()?;

        let primary = format!("{:.0}%", rate);

        let mut metadata = HashMap::new();
        metadata.insert(
            "total".to_string(),
            stats.tool_result_count.to_string(),
        );
        metadata.insert(
            "errors".to_string(),
            stats.tool_error_count.to_string(),
        );
        metadata.insert("rate".to_string(), format!("{:.2}", rate));

        Some(SegmentData {
            primary,
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::ToolSuccess
    }
}

use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct StopReasonSegment;

impl StopReasonSegment {
    pub fn new() -> Self {
        Self
    }

    fn icon_for_reason(reason: &str) -> &'static str {
        match reason {
            "end_turn" => "\u{f0791}",
            "tool_use" => "\u{f1064}",
            "max_tokens" => "\u{f0028}",
            "refusal" => "\u{f0159}",
            "stop_sequence" => "\u{f0666}",
            _ => "\u{f02d7}",
        }
    }
}

impl Segment for StopReasonSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let stats = input.transcript_stats()?;
        let reason = stats.last_stop_reason.as_ref()?.clone();

        let dynamic_icon = Self::icon_for_reason(&reason).to_string();

        let mut metadata = HashMap::new();
        metadata.insert("dynamic_icon".to_string(), dynamic_icon);
        metadata.insert("stop_reason".to_string(), reason.clone());

        Some(SegmentData {
            primary: reason,
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::StopReason
    }
}

use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct ApiDurationSegment;

impl ApiDurationSegment {
    pub fn new() -> Self {
        Self
    }

    fn format_duration(ms: u64) -> String {
        if ms < 1000 {
            format!("{}ms", ms)
        } else if ms < 60_000 {
            let seconds = ms / 1000;
            format!("{}s", seconds)
        } else if ms < 3_600_000 {
            let minutes = ms / 60_000;
            let seconds = (ms % 60_000) / 1000;
            if seconds == 0 {
                format!("{}m", minutes)
            } else {
                format!("{}m{}s", minutes, seconds)
            }
        } else {
            let hours = ms / 3_600_000;
            let minutes = (ms % 3_600_000) / 60_000;
            if minutes == 0 {
                format!("{}h", hours)
            } else {
                format!("{}h{}m", hours, minutes)
            }
        }
    }
}

impl Segment for ApiDurationSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let cost_data = input.cost.as_ref()?;
        let api_duration = cost_data.total_api_duration_ms?;

        let primary = Self::format_duration(api_duration);

        let mut metadata = HashMap::new();
        metadata.insert("api_duration_ms".to_string(), api_duration.to_string());

        Some(SegmentData {
            primary,
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::ApiDuration
    }
}

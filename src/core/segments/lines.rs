use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct LinesSegment;

impl LinesSegment {
    pub fn new() -> Self {
        Self
    }
}

impl Segment for LinesSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let cost_data = input.cost.as_ref()?;
        let added = cost_data.total_lines_added.unwrap_or(0);
        let removed = cost_data.total_lines_removed.unwrap_or(0);

        if added == 0 && removed == 0 {
            return None;
        }

        let primary = format!("+{} -{}", added, removed);

        let mut metadata = HashMap::new();
        metadata.insert("lines_added".to_string(), added.to_string());
        metadata.insert("lines_removed".to_string(), removed.to_string());

        Some(SegmentData {
            primary,
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Lines
    }
}

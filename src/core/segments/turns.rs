use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct TurnsSegment;

impl TurnsSegment {
    pub fn new() -> Self {
        Self
    }
}

impl Segment for TurnsSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let stats = input.transcript_stats()?;
        if stats.turn_count == 0 {
            return None;
        }

        let primary = format!("#{}", stats.turn_count);

        let mut metadata = HashMap::new();
        metadata.insert("turn_count".to_string(), stats.turn_count.to_string());

        Some(SegmentData {
            primary,
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Turns
    }
}

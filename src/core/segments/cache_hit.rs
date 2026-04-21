use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct CacheHitSegment;

impl CacheHitSegment {
    pub fn new() -> Self {
        Self
    }
}

impl Segment for CacheHitSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let stats = input.transcript_stats()?;
        let hit_rate = stats.cache_hit_rate()?;

        let primary = format!("{:.0}%", hit_rate);

        let mut metadata = HashMap::new();
        metadata.insert("hit_rate".to_string(), format!("{:.2}", hit_rate));
        metadata.insert(
            "cache_read".to_string(),
            stats.cache_read_tokens.to_string(),
        );
        metadata.insert(
            "cache_creation".to_string(),
            stats.cache_creation_tokens.to_string(),
        );
        metadata.insert("input".to_string(), stats.input_tokens.to_string());

        Some(SegmentData {
            primary,
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::CacheHit
    }
}

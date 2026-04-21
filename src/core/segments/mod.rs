pub mod api_duration;
pub mod cache_hit;
pub mod context_window;
pub mod cost;
pub mod directory;
pub mod git;
pub mod lines;
pub mod model;
pub mod output_style;
pub mod session;
pub mod stop_reason;
pub mod tool_success;
pub mod tools;
pub mod turns;
pub mod update;
pub mod usage;

use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

// New Segment trait for data collection only
pub trait Segment {
    fn collect(&self, input: &InputData) -> Option<SegmentData>;
    fn id(&self) -> SegmentId;
}

#[derive(Debug, Clone)]
pub struct SegmentData {
    pub primary: String,
    pub secondary: String,
    pub metadata: HashMap<String, String>,
}

// Re-export all segment types
pub use api_duration::ApiDurationSegment;
pub use cache_hit::CacheHitSegment;
pub use context_window::ContextWindowSegment;
pub use cost::CostSegment;
pub use directory::DirectorySegment;
pub use git::GitSegment;
pub use lines::LinesSegment;
pub use model::ModelSegment;
pub use output_style::OutputStyleSegment;
pub use session::SessionSegment;
pub use stop_reason::StopReasonSegment;
pub use tool_success::ToolSuccessSegment;
pub use tools::ToolsSegment;
pub use turns::TurnsSegment;
pub use update::UpdateSegment;
pub use usage::UsageSegment;

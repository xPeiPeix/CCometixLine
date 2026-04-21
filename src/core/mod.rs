pub mod segment_group;
pub mod segments;
pub mod statusline;
pub mod transcript;

pub use segment_group::SegmentGroup;
pub use statusline::{collect_all_segments, StatusLineGenerator};

use crate::config::SegmentId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentGroup {
    Identity,
    Place,
    Tokens,
    Quota,
    Time,
    Activity,
    Other,
}

/// Hard-coded inter-group order. Intra-group order is preserved from config
/// (Vec::sort_by_key is stable), so users can still reorder within a group.
pub const GROUP_ORDER: &[SegmentGroup] = &[
    SegmentGroup::Identity,
    SegmentGroup::Place,
    SegmentGroup::Tokens,
    SegmentGroup::Quota,
    SegmentGroup::Time,
    SegmentGroup::Activity,
    SegmentGroup::Other,
];

impl SegmentGroup {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Identity => "Identity",
            Self::Place => "Place",
            Self::Tokens => "Tokens",
            Self::Quota => "Quota",
            Self::Time => "Time",
            Self::Activity => "Activity",
            Self::Other => "Other",
        }
    }

    pub fn of(id: &SegmentId) -> Self {
        match id {
            SegmentId::Model | SegmentId::OutputStyle => Self::Identity,
            SegmentId::Directory | SegmentId::Git => Self::Place,
            SegmentId::ContextWindow | SegmentId::CacheHit => Self::Tokens,
            SegmentId::Usage | SegmentId::Cost => Self::Quota,
            SegmentId::Session | SegmentId::ApiDuration | SegmentId::Lines => Self::Time,
            SegmentId::Turns
            | SegmentId::Tools
            | SegmentId::StopReason
            | SegmentId::ToolSuccess => Self::Activity,
            SegmentId::Update => Self::Other,
        }
    }

    pub fn sort_key(&self) -> usize {
        GROUP_ORDER
            .iter()
            .position(|g| g == self)
            .expect("every SegmentGroup variant must appear in GROUP_ORDER")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_key_matches_group_order() {
        for (expected, g) in GROUP_ORDER.iter().enumerate() {
            assert_eq!(g.sort_key(), expected, "sort_key out of sync for {:?}", g);
        }
    }

    #[test]
    fn sort_key_is_exhaustive() {
        let all = [
            SegmentGroup::Identity,
            SegmentGroup::Place,
            SegmentGroup::Tokens,
            SegmentGroup::Quota,
            SegmentGroup::Time,
            SegmentGroup::Activity,
            SegmentGroup::Other,
        ];
        for g in all {
            let _ = g.sort_key(); // must not panic
        }
    }

    #[test]
    fn of_puts_every_segment_id_in_expected_group() {
        use SegmentGroup::*;
        use SegmentId::*;
        let cases = [
            (Model, Identity),
            (OutputStyle, Identity),
            (Directory, Place),
            (Git, Place),
            (ContextWindow, Tokens),
            (CacheHit, Tokens),
            (Usage, Quota),
            (Cost, Quota),
            (Session, Time),
            (ApiDuration, Time),
            (Lines, Time),
            (Turns, Activity),
            (Tools, Activity),
            (StopReason, Activity),
            (ToolSuccess, Activity),
            (Update, Other),
        ];
        for (id, expected) in cases {
            assert_eq!(SegmentGroup::of(&id), expected, "wrong group for {:?}", id);
        }
    }
}

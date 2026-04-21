use crate::config::Config;
use crate::core::{segment_group::GROUP_ORDER, SegmentGroup};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    SegmentList,
    Settings,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldSelection {
    Enabled,
    Icon,
    IconColor,
    TextColor,
    BackgroundColor,
    TextStyle,
    Options,
}

#[derive(Default)]
pub struct SegmentListComponent;

impl SegmentListComponent {
    pub fn new() -> Self {
        Self
    }

    pub fn render(
        &self,
        f: &mut Frame,
        area: Rect,
        config: &Config,
        selected_segment: usize,
        selected_panel: &Panel,
    ) {
        let mut items: Vec<ListItem> = Vec::new();
        let mut selected_list_index: Option<usize> = None;

        // Render segments bucketed by GROUP_ORDER without mutating the underlying
        // Vec — the user's on-disk order in config.toml is preserved, but the TUI
        // always shows tidy group sections.
        for group in GROUP_ORDER.iter().copied() {
            let mut group_has_items = false;
            for (vec_idx, segment) in config.segments.iter().enumerate() {
                if SegmentGroup::of(&segment.id) != group {
                    continue;
                }
                if !group_has_items {
                    items.push(ListItem::new(Line::from(vec![Span::styled(
                        format!("── {} ──", group.label()),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD),
                    )])));
                    group_has_items = true;
                }

                let is_selected =
                    vec_idx == selected_segment && *selected_panel == Panel::SegmentList;
                let enabled_marker = if segment.enabled { "●" } else { "○" };
                let segment_name = segment.id.display_name();

                if is_selected {
                    selected_list_index = Some(items.len());
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("  ▶ ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{} {}", enabled_marker, segment_name)),
                    ])));
                } else {
                    items.push(ListItem::new(format!(
                        "    {} {}",
                        enabled_marker, segment_name
                    )));
                }
            }
        }

        let segments_block = Block::default()
            .borders(Borders::ALL)
            .title("Segments")
            .border_style(if *selected_panel == Panel::SegmentList {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });
        let segments_list = List::new(items).block(segments_block);
        // Stateful render — ratatui auto-scrolls the visible window so the selected
        // item stays in view (prevents Stop Reason being clipped off-screen when
        // Preview steals a row).
        let mut state = ratatui::widgets::ListState::default();
        state.select(selected_list_index);
        f.render_stateful_widget(segments_list, area, &mut state);
    }
}

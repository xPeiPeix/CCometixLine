use crate::config::{AnsiColor, Config, SegmentConfig, StyleMode};
use crate::core::segments::SegmentData;

/// Strip ANSI escape sequences and return terminal display width.
///
/// Uses `UnicodeWidthStr` so Nerd Font icons, emoji, and CJK characters count as 2 cells
/// instead of 1 — otherwise the TUI preview wrapper under-estimates segment width and
/// ratatui's own Wrap re-splits a "full" line mid-grapheme (produces artefacts like
/// `to4ens` and truncates trailing segments off-screen).
///
/// Handles three escape-sequence classes so future hyperlink/title emission won't leak
/// into the cell count:
///   * CSI `\x1b[...FINAL_BYTE` (colors, cursor) — ends on 0x40..=0x7E
///   * OSC `\x1b]...BEL` or `...ESC\\` (hyperlinks, window title)
///   * Single-char ESC `\x1bX` for everything else (SS2/SS3/etc.)
fn visible_width(text: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    let mut visible = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            visible.push(ch);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // CSI — consume parameter/intermediate bytes until final byte.
                for c in chars.by_ref() {
                    if matches!(c, '\x40'..='\x7E') {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC — terminates on BEL (0x07) or ST (ESC \).
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' {
                        chars.next();
                        break;
                    }
                }
            }
            Some(_) | None => {
                // Single-character ESC sequence (or stray ESC at end of string).
            }
        }
    }

    UnicodeWidthStr::width(visible.as_str())
}

pub struct StatusLineGenerator {
    config: Config,
}

impl StatusLineGenerator {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn generate(&self, segments: Vec<(SegmentConfig, SegmentData)>) -> String {
        let enabled_segments: Vec<_> = segments
            .into_iter()
            .filter(|(config, _)| config.enabled)
            .collect();

        let mut output = Vec::new();
        let mut rendered_configs: Vec<SegmentConfig> = Vec::new();
        for (config, data) in enabled_segments.iter() {
            let rendered = self.render_segment(config, data);
            if !rendered.is_empty() {
                output.push(rendered);
                rendered_configs.push(config.clone());
            }
        }

        if output.is_empty() {
            return String::new();
        }

        // Handle Powerline arrow separators with color transition
        if self.config.style.separator == "\u{e0b0}" {
            self.join_with_powerline_arrows(&output, &enabled_segments)
        } else {
            // For all other separators, use white color and per-segment override
            self.join_with_white_separators(&output, &rendered_configs)
        }
    }

    /// Generate TUI-optimized text with intelligent wrapping by segment for preview
    pub fn generate_for_tui_preview(
        &self,
        segments: Vec<(SegmentConfig, SegmentData)>,
        max_width: u16,
    ) -> ratatui::text::Text<'_> {
        use ansi_to_tui::IntoText;
        use ratatui::text::{Line, Span, Text};

        let enabled_segments: Vec<_> = segments
            .into_iter()
            .filter(|(config, _)| config.enabled)
            .collect();

        if enabled_segments.is_empty() {
            return Text::from(vec![Line::default()]);
        }

        // Render each segment individually
        let mut rendered_segments = Vec::new();
        let mut segment_configs = Vec::new();

        for (config, data) in &enabled_segments {
            let rendered = self.render_segment(config, data);
            if !rendered.is_empty() {
                rendered_segments.push(rendered);
                segment_configs.push(config.clone());
            }
        }

        if rendered_segments.is_empty() {
            return Text::from(vec![Line::default()]);
        }

        // Pre-calculate separators between segments (uses decide_separator for
        // user-override / intra-group ` · ` / inter-group global — same as runtime)
        let mut separators = Vec::new();
        for i in 0..rendered_segments.len().saturating_sub(1) {
            let separator = if self.config.style.separator == "\u{e0b0}" {
                // Powerline arrows with color transition
                let prev_bg = segment_configs
                    .get(i)
                    .and_then(|config| config.colors.background.as_ref());
                let curr_bg = segment_configs
                    .get(i + 1)
                    .and_then(|config| config.colors.background.as_ref());
                self.create_powerline_arrow(prev_bg, curr_bg)
            } else {
                let sep = self.decide_separator(&segment_configs[i + 1], &segment_configs[i]);
                format!("\x1b[37m{}\x1b[0m", sep)
            };
            separators.push(separator);
        }

        // Intelligent line wrapping by segment
        let mut lines: Vec<String> = Vec::new();
        let mut current_line = String::new();
        let mut current_width = 0usize;
        let max_w = max_width as usize;

        for i in 0..rendered_segments.len() {
            let segment = &rendered_segments[i];
            let segment_width = visible_width(segment);

            // Check if adding this segment would exceed max_width
            if current_width > 0 && current_width + segment_width > max_w {
                // Current line would overflow, start a new line
                lines.push(current_line.clone());
                current_line.clear();
                current_width = 0;
            }

            // Add the segment to current line
            current_line.push_str(segment);
            current_width += segment_width;

            // Handle separator if not the last segment
            if i < separators.len() {
                let separator = &separators[i];
                let separator_width = visible_width(separator);

                // Check if next segment exists
                if i + 1 < rendered_segments.len() {
                    let next_segment = &rendered_segments[i + 1];
                    let next_width = visible_width(next_segment);

                    // Check if separator AND next segment both fit
                    if current_width + separator_width + next_width <= max_w {
                        // Both fit, add separator and continue on same line
                        current_line.push_str(separator);
                        current_width += separator_width;
                    } else {
                        // Separator and/or next segment don't fit
                        // Don't add separator, just break line
                        lines.push(current_line.clone());
                        current_line.clear();
                        current_width = 0;
                    }
                }
            }
        }

        // Add the last line if it's not empty
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        // Convert string lines to ratatui Text
        let mut tui_lines = Vec::new();
        for line in lines {
            if let Ok(text) = line.into_text() {
                for tui_line in text.lines {
                    tui_lines.push(tui_line);
                }
            } else {
                tui_lines.push(Line::from(vec![Span::raw(line)]));
            }
        }

        // Ensure we have at least one line
        if tui_lines.is_empty() {
            tui_lines.push(Line::default());
        }

        Text::from(tui_lines)
    }

    fn render_segment(&self, config: &SegmentConfig, data: &SegmentData) -> String {
        let icon = if let Some(dynamic_icon) = data.metadata.get("dynamic_icon") {
            dynamic_icon.clone()
        } else {
            self.get_icon(config)
        };

        // Apply background color to the entire segment if set
        if let Some(bg_color) = &config.colors.background {
            let bg_code = self.apply_background_color(bg_color);

            // Build the entire segment content first
            let icon_colored = if let Some(icon_color) = &config.colors.icon {
                self.apply_color(&icon, Some(icon_color))
                    .replace("\x1b[0m", "")
            } else {
                icon.clone()
            };

            let text_styled = self
                .apply_style(
                    &data.primary,
                    config.colors.text.as_ref(),
                    config.styles.text_bold,
                )
                .replace("\x1b[0m", "");

            let mut segment_content = format!(" {} {} ", icon_colored, text_styled);

            if !data.secondary.is_empty() {
                let secondary_styled = self
                    .apply_style(
                        &data.secondary,
                        config.colors.text.as_ref(),
                        config.styles.text_bold,
                    )
                    .replace("\x1b[0m", "");
                segment_content.push_str(&format!("{} ", secondary_styled));
            }

            // Apply background to the entire content and reset at the end
            format!("{}{}\x1b[49m", bg_code, segment_content)
        } else {
            // No background color, use original logic
            let icon_colored = self.apply_color(&icon, config.colors.icon.as_ref());
            let text_styled = self.apply_style(
                &data.primary,
                config.colors.text.as_ref(),
                config.styles.text_bold,
            );

            let mut segment = format!("{} {}", icon_colored, text_styled);

            if !data.secondary.is_empty() {
                segment.push_str(&format!(
                    " {}",
                    self.apply_style(
                        &data.secondary,
                        config.colors.text.as_ref(),
                        config.styles.text_bold
                    )
                ));
            }

            segment
        }
    }

    fn get_icon(&self, config: &SegmentConfig) -> String {
        match self.config.style.mode {
            StyleMode::Plain => config.icon.plain.clone(),
            StyleMode::NerdFont => config.icon.nerd_font.clone(),
            StyleMode::Powerline => config.icon.nerd_font.clone(), // Future: use Powerline icons
        }
    }

    fn apply_color(&self, text: &str, color: Option<&AnsiColor>) -> String {
        match color {
            Some(AnsiColor::Color16 { c16 }) => {
                let code = if *c16 < 8 { 30 + c16 } else { 90 + (c16 - 8) };
                format!("\x1b[{}m{}\x1b[0m", code, text)
            }
            Some(AnsiColor::Color256 { c256 }) => {
                format!("\x1b[38;5;{}m{}\x1b[0m", c256, text)
            }
            Some(AnsiColor::Rgb { r, g, b }) => {
                format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, text)
            }
            None => text.to_string(),
        }
    }

    fn apply_style(&self, text: &str, color: Option<&AnsiColor>, bold: bool) -> String {
        let mut codes = Vec::new();

        // Add style codes
        if bold {
            codes.push("1".to_string()); // Bold: \x1b[1m
        }

        // Add color codes
        match color {
            Some(AnsiColor::Color16 { c16 }) => {
                let color_code = if *c16 < 8 { 30 + c16 } else { 90 + (c16 - 8) };
                codes.push(color_code.to_string());
            }
            Some(AnsiColor::Color256 { c256 }) => {
                codes.push("38".to_string());
                codes.push("5".to_string());
                codes.push(c256.to_string());
            }
            Some(AnsiColor::Rgb { r, g, b }) => {
                codes.push("38".to_string());
                codes.push("2".to_string());
                codes.push(r.to_string());
                codes.push(g.to_string());
                codes.push(b.to_string());
            }
            None => {}
        }

        if codes.is_empty() {
            text.to_string()
        } else {
            format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
        }
    }

    fn apply_background_color(&self, color: &AnsiColor) -> String {
        match color {
            AnsiColor::Color16 { c16 } => {
                let code = if *c16 < 8 { 40 + c16 } else { 100 + (c16 - 8) };
                format!("\x1b[{}m", code)
            }
            AnsiColor::Color256 { c256 } => {
                format!("\x1b[48;5;{}m", c256)
            }
            AnsiColor::Rgb { r, g, b } => {
                format!("\x1b[48;2;{};{};{}m", r, g, b)
            }
        }
    }

    /// Decide the separator string between two adjacent segments.
    ///
    /// Priority (first matching branch wins):
    ///   1. `cur.options.separator_before` — explicit user override.
    ///   2. Theme's `style.separator` is empty (pure-bubble themes like powerline
    ///      variants and nord) → empty string; background-color transitions are
    ///      the separator and injecting ` · ` would break the visual design.
    ///   3. Same `SegmentGroup` as prev → ` · ` (intra-group).
    ///   4. Different group → global `style.separator` (inter-group).
    ///
    /// Single source of truth — both runtime statusline and TUI preview use this.
    fn decide_separator(&self, cur: &SegmentConfig, prev: &SegmentConfig) -> String {
        use crate::core::SegmentGroup;
        if let Some(v) = cur
            .options
            .get("separator_before")
            .and_then(|v| v.as_str())
        {
            return v.to_string();
        }
        // Pure-bubble themes (e.g., powerline variants with empty separator) rely on
        // background-color transitions; injecting ` · ` inside groups would break the
        // visual design. Keep separators empty across the board for these themes.
        if self.config.style.separator.is_empty() {
            return String::new();
        }
        if SegmentGroup::of(&cur.id) == SegmentGroup::of(&prev.id) {
            " · ".to_string()
        } else {
            self.config.style.separator.clone()
        }
    }

    /// Join segments with white separators (non-Powerline).
    fn join_with_white_separators(
        &self,
        rendered_segments: &[String],
        configs: &[SegmentConfig],
    ) -> String {
        if rendered_segments.is_empty() {
            return String::new();
        }

        let mut result = rendered_segments[0].clone();
        for (i, rendered) in rendered_segments.iter().enumerate().skip(1) {
            let sep = self.decide_separator(&configs[i], &configs[i - 1]);
            let white_sep = format!("\x1b[37m{}\x1b[0m", sep);
            result.push_str(&white_sep);
            result.push_str(rendered);
        }
        result
    }

    /// Join segments with Powerline arrow separators with proper color transitions
    fn join_with_powerline_arrows(
        &self,
        rendered_segments: &[String],
        segment_configs: &[(SegmentConfig, SegmentData)],
    ) -> String {
        if rendered_segments.is_empty() {
            return String::new();
        }

        if rendered_segments.len() == 1 {
            return rendered_segments[0].clone();
        }

        let mut result = rendered_segments[0].clone();

        for (i, _) in rendered_segments.iter().enumerate().skip(1) {
            let prev_bg = segment_configs
                .get(i - 1)
                .and_then(|(config, _)| config.colors.background.as_ref());
            let curr_bg = segment_configs
                .get(i)
                .and_then(|(config, _)| config.colors.background.as_ref());

            // Create Powerline arrow with color transition
            let arrow = self.create_powerline_arrow(prev_bg, curr_bg);

            result.push_str(&arrow);
            result.push_str(&rendered_segments[i]);
        }

        // Reset colors at the end
        result.push_str("\x1b[0m");
        result
    }

    /// Create a Powerline arrow with proper color transition
    fn create_powerline_arrow(
        &self,
        prev_bg: Option<&AnsiColor>,
        curr_bg: Option<&AnsiColor>,
    ) -> String {
        let arrow_char = "\u{e0b0}";

        match (prev_bg, curr_bg) {
            (Some(prev), Some(curr)) => {
                // Arrow foreground = previous segment's background
                // Arrow background = current segment's background
                let fg_code = self.color_to_foreground_code(prev);
                let bg_code = self.apply_background_color(curr);
                format!("{}{}{}\x1b[0m", bg_code, fg_code, arrow_char)
            }
            (Some(prev), None) => {
                // Previous segment has background, current doesn't
                let fg_code = self.color_to_foreground_code(prev);
                format!("{}{}\x1b[0m", fg_code, arrow_char)
            }
            (None, Some(curr)) => {
                // Current segment has background, previous doesn't
                let bg_code = self.apply_background_color(curr);
                format!("{}{}\x1b[0m", bg_code, arrow_char)
            }
            (None, None) => {
                // Neither segment has background color
                arrow_char.to_string()
            }
        }
    }

    /// Convert AnsiColor to foreground color code
    fn color_to_foreground_code(&self, color: &AnsiColor) -> String {
        match color {
            AnsiColor::Color16 { c16 } => {
                let code = if *c16 < 8 { 30 + c16 } else { 90 + (c16 - 8) };
                format!("\x1b[{}m", code)
            }
            AnsiColor::Color256 { c256 } => {
                format!("\x1b[38;5;{}m", c256)
            }
            AnsiColor::Rgb { r, g, b } => {
                format!("\x1b[38;2;{};{};{}m", r, g, b)
            }
        }
    }
}

pub fn collect_all_segments(
    config: &Config,
    input: &crate::config::InputData,
) -> Vec<(SegmentConfig, SegmentData)> {
    use crate::core::segments::*;

    let mut results = Vec::new();

    for segment_config in &config.segments {
        // Skip disabled segments to avoid unnecessary API requests
        if !segment_config.enabled {
            continue;
        }

        let segment_data = match segment_config.id {
            crate::config::SegmentId::Model => {
                let segment = ModelSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Directory => {
                let segment = DirectorySegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Git => {
                let show_sha = segment_config
                    .options
                    .get("show_sha")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let segment = GitSegment::new().with_sha(show_sha);
                segment.collect(input)
            }
            crate::config::SegmentId::ContextWindow => {
                let segment = ContextWindowSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Usage => {
                let segment = UsageSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Cost => {
                let segment = CostSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Session => {
                let segment = SessionSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::OutputStyle => {
                let segment = OutputStyleSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Update => {
                let segment = UpdateSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::ApiDuration => {
                let segment = ApiDurationSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Lines => {
                let segment = LinesSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::CacheHit => {
                let segment = CacheHitSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Turns => {
                let segment = TurnsSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::Tools => {
                let segment = ToolsSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::StopReason => {
                let segment = StopReasonSegment::new();
                segment.collect(input)
            }
            crate::config::SegmentId::ToolSuccess => {
                let segment = ToolSuccessSegment::new();
                segment.collect(input)
            }
        };

        if let Some(data) = segment_data {
            results.push((segment_config.clone(), data));
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ColorConfig, IconConfig, SegmentConfig, SegmentId, StyleConfig, StyleMode,
        TextStyleConfig,
    };
    use std::collections::HashMap;

    fn seg(id: SegmentId, sep_override: Option<&str>) -> SegmentConfig {
        let mut options = HashMap::new();
        if let Some(s) = sep_override {
            options.insert(
                "separator_before".to_string(),
                serde_json::Value::String(s.to_string()),
            );
        }
        SegmentConfig {
            id,
            enabled: true,
            icon: IconConfig {
                plain: String::new(),
                nerd_font: String::new(),
            },
            colors: ColorConfig {
                icon: None,
                text: None,
                background: None,
            },
            styles: TextStyleConfig::default(),
            options,
        }
    }

    fn gen_with_sep(sep: &str) -> StatusLineGenerator {
        StatusLineGenerator::new(Config {
            style: StyleConfig {
                mode: StyleMode::NerdFont,
                separator: sep.to_string(),
            },
            segments: Vec::new(),
            theme: String::new(),
        })
    }

    #[test]
    fn decide_separator_intra_group_uses_middot() {
        let g = gen_with_sep(" | ");
        // Model + OutputStyle both live in Identity.
        let cur = seg(SegmentId::OutputStyle, None);
        let prev = seg(SegmentId::Model, None);
        assert_eq!(g.decide_separator(&cur, &prev), " · ");
    }

    #[test]
    fn decide_separator_inter_group_uses_theme_separator() {
        let g = gen_with_sep(" | ");
        // Cost (Quota) -> Session (Time) crosses groups.
        let cur = seg(SegmentId::Session, None);
        let prev = seg(SegmentId::Cost, None);
        assert_eq!(g.decide_separator(&cur, &prev), " | ");
    }

    #[test]
    fn decide_separator_user_override_wins() {
        let g = gen_with_sep(" | ");
        let cur = seg(SegmentId::Session, Some(" x "));
        let prev = seg(SegmentId::Cost, None);
        assert_eq!(g.decide_separator(&cur, &prev), " x ");
    }

    #[test]
    fn decide_separator_bubble_theme_returns_empty_even_intra_group() {
        let g = gen_with_sep("");
        let cur = seg(SegmentId::OutputStyle, None);
        let prev = seg(SegmentId::Model, None);
        assert_eq!(g.decide_separator(&cur, &prev), "");
    }

    #[test]
    fn decide_separator_bubble_theme_user_override_still_wins() {
        // Even on a bubble theme, explicit separator_before should not be silenced.
        let g = gen_with_sep("");
        let cur = seg(SegmentId::Session, Some(" · "));
        let prev = seg(SegmentId::Cost, None);
        assert_eq!(g.decide_separator(&cur, &prev), " · ");
    }

    #[test]
    fn visible_width_counts_plain_ascii() {
        assert_eq!(visible_width("hello"), 5);
    }

    #[test]
    fn visible_width_strips_csi_colors() {
        assert_eq!(visible_width("\x1b[31mred\x1b[0m"), 3);
        assert_eq!(visible_width("\x1b[38;5;214m214-color\x1b[0m"), 9);
    }

    #[test]
    fn visible_width_strips_osc_hyperlink() {
        // OSC 8 hyperlink: ESC ] 8 ; ; URL ESC \ TEXT ESC ] 8 ; ; ESC \
        let hyperlinked = "\x1b]8;;https://example.com\x1b\\click\x1b]8;;\x1b\\";
        assert_eq!(visible_width(hyperlinked), 5);
    }

    #[test]
    fn visible_width_counts_nerd_font_icons_as_two_cells() {
        use unicode_width::UnicodeWidthStr;
        // Most Nerd Font PUA codepoints have width 2; assert on a known glyph.
        let icon = "\u{f024b}"; // folder icon used by Directory segment
        assert_eq!(visible_width(icon), UnicodeWidthStr::width(icon));
    }

    #[test]
    fn visible_width_handles_cjk() {
        // CJK ideographs are width 2 each.
        assert_eq!(visible_width("中文"), 4);
    }

    #[test]
    fn normalize_segment_order_groups_by_group_order() {
        use crate::core::SegmentGroup;
        let mut cfg = Config {
            style: StyleConfig {
                mode: StyleMode::NerdFont,
                separator: " | ".to_string(),
            },
            // Intentionally scrambled: Activity -> Identity -> Time -> Place
            segments: vec![
                seg(SegmentId::Tools, None),
                seg(SegmentId::Model, None),
                seg(SegmentId::Session, None),
                seg(SegmentId::Directory, None),
            ],
            theme: String::new(),
        };
        cfg.normalize_segment_order();
        let groups: Vec<_> = cfg
            .segments
            .iter()
            .map(|s| SegmentGroup::of(&s.id))
            .collect();
        assert_eq!(
            groups,
            vec![
                SegmentGroup::Identity,
                SegmentGroup::Place,
                SegmentGroup::Time,
                SegmentGroup::Activity,
            ]
        );
    }

    #[test]
    fn normalize_segment_order_preserves_intra_group_order() {
        let mut cfg = Config {
            style: StyleConfig {
                mode: StyleMode::NerdFont,
                separator: " | ".to_string(),
            },
            // Both Time group; user explicitly put ApiDuration before Session.
            // Stable sort must preserve that ordering.
            segments: vec![
                seg(SegmentId::ApiDuration, None),
                seg(SegmentId::Session, None),
                seg(SegmentId::Lines, None),
            ],
            theme: String::new(),
        };
        cfg.normalize_segment_order();
        let ids: Vec<_> = cfg.segments.iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            vec![SegmentId::ApiDuration, SegmentId::Session, SegmentId::Lines]
        );
    }
}

use crate::config::{
    AnsiColor, ColorConfig, IconConfig, SegmentConfig, SegmentId, TextStyleConfig,
};
use crate::core::SegmentGroup;
use std::collections::HashMap;

fn group_colors(
    group: SegmentGroup,
) -> (AnsiColor, AnsiColor, Option<AnsiColor>) {
    match group {
        SegmentGroup::Identity => (
            AnsiColor::Rgb { r: 46, g: 52, b: 64 },
            AnsiColor::Rgb { r: 46, g: 52, b: 64 },
            Some(AnsiColor::Rgb { r: 136, g: 192, b: 208 }),
        ),
        SegmentGroup::Place => (
            AnsiColor::Rgb { r: 46, g: 52, b: 64 },
            AnsiColor::Rgb { r: 46, g: 52, b: 64 },
            Some(AnsiColor::Rgb { r: 163, g: 190, b: 140 }),
        ),
        SegmentGroup::Tokens => (
            AnsiColor::Rgb { r: 46, g: 52, b: 64 },
            AnsiColor::Rgb { r: 46, g: 52, b: 64 },
            Some(AnsiColor::Rgb { r: 180, g: 142, b: 173 }),
        ),
        SegmentGroup::Quota => (
            AnsiColor::Rgb { r: 46, g: 52, b: 64 },
            AnsiColor::Rgb { r: 46, g: 52, b: 64 },
            Some(AnsiColor::Rgb { r: 235, g: 203, b: 139 }),
        ),
        SegmentGroup::Time => (
            AnsiColor::Rgb { r: 236, g: 239, b: 244 },
            AnsiColor::Rgb { r: 236, g: 239, b: 244 },
            Some(AnsiColor::Rgb { r: 129, g: 161, b: 193 }),
        ),
        SegmentGroup::Activity => (
            AnsiColor::Rgb { r: 236, g: 239, b: 244 },
            AnsiColor::Rgb { r: 236, g: 239, b: 244 },
            Some(AnsiColor::Rgb { r: 191, g: 97, b: 106 }),
        ),
        SegmentGroup::Other => (
            AnsiColor::Color16 { c16: 7 },
            AnsiColor::Color16 { c16: 7 },
            None,
        ),
    }
}


pub fn model_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Identity);
    SegmentConfig {
        id: SegmentId::Model,
        enabled: true,
        icon: IconConfig {
            plain: "🤖".to_string(),
            nerd_font: "\u{e26d}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn directory_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Place);
    SegmentConfig {
        id: SegmentId::Directory,
        enabled: true,
        icon: IconConfig {
            plain: "📁".to_string(),
            nerd_font: "\u{f024b}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn git_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Place);
    SegmentConfig {
        id: SegmentId::Git,
        enabled: true,
        icon: IconConfig {
            plain: "🌿".to_string(),
            nerd_font: "\u{f02a2}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: {
            let mut opts = HashMap::new();
            opts.insert("show_sha".to_string(), serde_json::Value::Bool(false));
            opts
        },
    }
}

pub fn context_window_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Tokens);
    SegmentConfig {
        id: SegmentId::ContextWindow,
        enabled: true,
        icon: IconConfig {
            plain: "⚡️".to_string(),
            nerd_font: "\u{f49b}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn cost_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Quota);
    SegmentConfig {
        id: SegmentId::Cost,
        enabled: false,
        icon: IconConfig {
            plain: "💰".to_string(),
            nerd_font: "\u{eec1}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn session_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Time);
    SegmentConfig {
        id: SegmentId::Session,
        enabled: false,
        icon: IconConfig {
            plain: "⏱️".to_string(),
            nerd_font: "\u{f19bb}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn output_style_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Identity);
    SegmentConfig {
        id: SegmentId::OutputStyle,
        enabled: false,
        icon: IconConfig {
            plain: "🎯".to_string(),
            nerd_font: "\u{f12f5}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn usage_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Quota);
    SegmentConfig {
        id: SegmentId::Usage,
        enabled: false,
        icon: IconConfig {
            plain: "📊".to_string(),
            nerd_font: "\u{f0a9e}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: {
            let mut opts = HashMap::new();
            opts.insert(
                "api_base_url".to_string(),
                serde_json::Value::String("https://api.anthropic.com".to_string()),
            );
            opts.insert(
                "cache_duration".to_string(),
                serde_json::Value::Number(180.into()),
            );
            opts.insert("timeout".to_string(), serde_json::Value::Number(2.into()));
            opts
        },
    }
}

pub fn api_duration_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Time);
    SegmentConfig {
        id: SegmentId::ApiDuration,
        enabled: false,
        icon: IconConfig {
            plain: "🤖".to_string(),
            nerd_font: "\u{f06a9}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn lines_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Time);
    SegmentConfig {
        id: SegmentId::Lines,
        enabled: false,
        icon: IconConfig {
            plain: "±".to_string(),
            nerd_font: "\u{f0992}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn cache_hit_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Tokens);
    SegmentConfig {
        id: SegmentId::CacheHit,
        enabled: false,
        icon: IconConfig {
            plain: "💾".to_string(),
            nerd_font: "\u{f0aa9}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn turns_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Activity);
    SegmentConfig {
        id: SegmentId::Turns,
        enabled: false,
        icon: IconConfig {
            plain: "💬".to_string(),
            nerd_font: "\u{f0369}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn tools_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Activity);
    SegmentConfig {
        id: SegmentId::Tools,
        enabled: false,
        icon: IconConfig {
            plain: "🔧".to_string(),
            nerd_font: "\u{f1064}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn stop_reason_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Activity);
    SegmentConfig {
        id: SegmentId::StopReason,
        enabled: false,
        icon: IconConfig {
            plain: "?".to_string(),
            nerd_font: "\u{f02d6}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

pub fn tool_success_segment() -> SegmentConfig {
    let (icon, text, bg) = group_colors(SegmentGroup::Activity);
    SegmentConfig {
        id: SegmentId::ToolSuccess,
        enabled: false,
        icon: IconConfig {
            plain: "✓".to_string(),
            nerd_font: "\u{f05e0}".to_string(),
        },
        colors: ColorConfig {
            icon: Some(icon),
            text: Some(text),
            background: bg,
        },
        styles: TextStyleConfig::default(),
        options: HashMap::new(),
    }
}

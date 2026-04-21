use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ccline")]
#[command(version, about = "High-performance Claude Code StatusLine")]
pub struct Cli {
    /// Enter TUI configuration mode
    #[arg(short = 'c', long = "config")]
    pub config: bool,

    /// Set theme
    #[arg(short = 't', long = "theme")]
    pub theme: Option<String>,

    /// Patch Claude Code cli.js to disable context warnings
    #[arg(long = "patch")]
    pub patch: Option<String>,

    /// Overwrite all built-in theme files under ~/.claude/ccline/themes/
    /// with the latest defaults compiled into this binary. Use after upgrading
    /// when you want new palettes to take effect.
    #[arg(long = "reset-themes")]
    pub reset_themes: bool,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

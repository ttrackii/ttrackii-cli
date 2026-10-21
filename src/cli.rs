use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "ttrackii", version, about = "Track time from your terminal")]
pub struct Cli {
    /// API origin. HTTP is accepted only for localhost development.
    #[arg(
        long,
        global = true,
        env = "TTRACKII_API_URL",
        default_value = "https://app.ttrackii.com"
    )]
    pub api_url: String,

    /// Emit stable machine-readable JSON.
    #[arg(long, global = true)]
    pub json: bool,

    /// Disable ANSI colors (also honored through `NO_COLOR`).
    #[arg(long, global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Authenticate in your web browser.
    Login {
        /// Print the verification URL without opening a browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Revoke the active session and remove the stored credential.
    Logout,
    /// Show the authenticated account.
    Whoami,
    /// List or select the default workspace.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Start a timer, stopping any timer already running.
    Start(StartArgs),
    /// Stop the currently running timer.
    Stop,
    /// Show whether a timer is running and its elapsed duration.
    Status,
    /// Show a time report.
    Report(ReportArgs),
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// List accessible workspaces.
    List,
    /// Set the default workspace.
    Use { workspace: String },
}

#[derive(Debug, Args)]
pub struct StartArgs {
    /// Description for the timer.
    pub task: Option<String>,

    /// Project name or ID.
    #[arg(long)]
    pub project: Option<String>,

    /// Tag name or ID.
    #[arg(long)]
    pub tag: Option<String>,

    /// Workspace name, slug, ID, or "personal".
    #[arg(long)]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ReportArgs {
    /// Workspace name, slug, ID, or "personal".
    #[arg(long)]
    pub workspace: Option<String>,

    /// A convenient date range. Defaults to this week.
    #[arg(long, value_enum, conflicts_with_all = ["from", "to"])]
    pub period: Option<ReportPeriod>,

    /// First local calendar date, inclusive (YYYY-MM-DD).
    #[arg(long, value_parser = parse_date, requires = "to", conflicts_with = "period")]
    pub from: Option<NaiveDate>,

    /// Last local calendar date, inclusive (YYYY-MM-DD).
    #[arg(long, value_parser = parse_date, requires = "from", conflicts_with = "period")]
    pub to: Option<NaiveDate>,

    /// Include all team members. Requires workspace admin access.
    #[arg(long)]
    pub all_members: bool,

    /// Show only totals, without the per-entry list.
    #[arg(long)]
    pub summary: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReportPeriod {
    Today,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    Quarter,
    Year,
}

fn parse_date(value: &str) -> std::result::Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("{value:?} is not a valid YYYY-MM-DD date"))
}

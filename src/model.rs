use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub kind: WorkspaceKind,
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default = "default_true")]
    pub writable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceKind {
    Personal,
    Team,
}

impl std::fmt::Display for WorkspaceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Personal => formatter.write_str("personal"),
            Self::Team => formatter.write_str("team"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub user: User,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesResponse {
    #[serde(default)]
    pub projects: Vec<Resource>,
    #[serde(default, alias = "groups", alias = "taskGroups")]
    pub tags: Vec<Resource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerEntry {
    pub id: String,
    #[serde(alias = "task")]
    pub task_name: String,
    pub workspace_id: String,
    pub workspace_name: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default, alias = "taskGroupId")]
    pub tag_id: Option<String>,
    #[serde(default, alias = "taskGroupName")]
    pub tag_name: Option<String>,
    #[serde(default)]
    pub start_time: i64,
    #[serde(default)]
    pub end_time: Option<i64>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub is_running: bool,
    #[serde(default)]
    pub is_billable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerResponse {
    pub running: bool,
    #[serde(default)]
    pub entry: Option<TimerEntry>,
    pub as_of: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerRequest {
    pub workspace_id: String,
    pub task_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    #[serde(default)]
    pub total_duration_ms: u64,
    pub entry_count: u64,
    #[serde(default)]
    pub running_entry_count: u64,
    #[serde(default)]
    pub by_project: Vec<ReportGroup>,
    #[serde(default, alias = "byTaskGroup")]
    pub by_tag: Vec<ReportGroup>,
    #[serde(default)]
    pub by_member: Vec<MemberReportGroup>,
    #[serde(default)]
    pub billable_totals: Vec<BillableTotal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportGroup {
    pub name: String,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub entries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberReportGroup {
    pub user_id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub entries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillableTotal {
    pub currency: String,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportEntry {
    pub id: String,
    pub member_id: String,
    #[serde(default)]
    pub member_name: Option<String>,
    #[serde(default)]
    pub member_email: Option<String>,
    #[serde(alias = "task")]
    pub task_name: String,
    pub start_time: i64,
    #[serde(default)]
    pub end_time: Option<i64>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default, alias = "taskGroupName")]
    pub tag_name: Option<String>,
    #[serde(default)]
    pub is_billable: bool,
    #[serde(default)]
    pub is_running: bool,
    #[serde(default)]
    pub billing_rate_minor: Option<i64>,
    #[serde(default)]
    pub billing_currency: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportResponse {
    pub start_date: String,
    pub end_date: String,
    pub workspace: Workspace,
    pub scope: String,
    pub summary: ReportSummary,
    #[serde(default)]
    pub entries: Vec<ReportEntry>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

const fn default_true() -> bool {
    true
}

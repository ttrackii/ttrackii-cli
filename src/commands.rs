use std::env;

use chrono::{Datelike, Days, Local, Months, NaiveDate, SecondsFormat, Utc};
use reqwest::StatusCode;

use crate::{
    api::ApiClient,
    auth::{DeviceAuth, TokenManager, TokenSource, open_browser, verification_url},
    cli::{Cli, Command, ReportArgs, ReportPeriod, StartArgs, WorkspaceCommand},
    config::{Config, ConfigStore},
    error::{CliError, Result},
    model::{
        Resource, SessionResponse, StartTimerRequest, TimerResponse, Workspace, WorkspaceKind,
    },
    output,
};

pub async fn run(cli: Cli) -> Result<()> {
    let token_manager = TokenManager::default();
    let result = run_with(cli, &token_manager, ConfigStore::system()?).await;
    if matches!(result, Err(CliError::AuthRequired)) {
        if let Ok(Some(credential)) = token_manager.load() {
            if credential.source == TokenSource::Keyring {
                let _ = token_manager.delete_stored();
            }
        }
    }
    result
}

async fn run_with(cli: Cli, tokens: &TokenManager, config_store: ConfigStore) -> Result<()> {
    let json = cli.json;
    output::set_colors_enabled(!cli.no_color && env::var_os("NO_COLOR").is_none());
    match cli.command {
        Command::Login { no_open } => login(&cli.api_url, no_open, json, tokens).await,
        Command::Logout => logout(&cli.api_url, json, tokens).await,
        command => {
            let credential = tokens.load()?.ok_or(CliError::AuthRequired)?;
            let api = ApiClient::new(&cli.api_url, Some(credential.token))?;
            match command {
                Command::Whoami => whoami(&api, json).await,
                Command::Workspace { command } => {
                    workspace(&api, command, json, &config_store).await
                }
                Command::Start(args) => start(&api, args, json, &config_store).await,
                Command::Stop => stop(&api, json).await,
                Command::Status => status(&api, json).await,
                Command::Report(args) => report(&api, &args, json, &config_store).await,
                Command::Login { .. } | Command::Logout => unreachable!(),
            }
        }
    }
}

async fn login(api_url: &str, no_open: bool, json: bool, tokens: &TokenManager) -> Result<()> {
    if matches!(tokens.load()?, Some(credential) if credential.source == TokenSource::Environment) {
        return Err(CliError::Authentication(
            "TTRACKII_TOKEN is set; remove it before browser login".into(),
        ));
    }
    let auth = DeviceAuth::new(api_url)?;
    let code = auth.request_code().await?;
    if json {
        output::json(&serde_json::json!({
            "status": "authorization_required",
            "userCode": code.user_code,
            "verificationUri": code.verification_uri,
            "verificationUriComplete": code.verification_uri_complete,
            "expiresIn": code.expires_in,
        }))?;
    } else {
        println!("Open this URL to authenticate:");
        println!("{}", verification_url(&code));
        println!("Code: {}", code.user_code);
        println!("Enable Settings → API & CLI → Allow API access before approving.");
    }
    if !no_open {
        if let Err(error) = open_browser(&code) {
            eprintln!("Warning: {error}");
        }
    }
    let token = auth.poll_token(&code).await?;
    if let Err(error) = tokens.save(&token) {
        if let Ok(api) = ApiClient::new(api_url, Some(token)) {
            let _ = api.revoke().await;
        }
        return Err(error);
    }
    if json {
        output::json(&serde_json::json!({ "status": "authenticated" }))
    } else {
        println!(
            "{} Authenticated — welcome to ttrackii!",
            output::paint("1;32", "✓")
        );
        Ok(())
    }
}

async fn logout(api_url: &str, json: bool, tokens: &TokenManager) -> Result<()> {
    if let Some(credential) = tokens.load()? {
        let api = ApiClient::new(api_url, Some(credential.token))?;
        match api.revoke().await {
            Ok(()) | Err(CliError::AuthRequired) => {}
            Err(error) => return Err(error),
        }
    }
    tokens.delete_stored()?;
    if json {
        output::json(&serde_json::json!({ "status": "logged_out" }))
    } else {
        println!(
            "{} Logged out — see you next time!",
            output::paint("1;36", "✓")
        );
        Ok(())
    }
}

async fn whoami(api: &ApiClient, json: bool) -> Result<()> {
    let session = api.session().await?;
    if json {
        output::json(&session)
    } else {
        output::session(&session);
        Ok(())
    }
}

async fn workspace(
    api: &ApiClient,
    command: WorkspaceCommand,
    json: bool,
    config_store: &ConfigStore,
) -> Result<()> {
    let session = api.session().await?;
    match command {
        WorkspaceCommand::List => {
            let config = config_store.load()?;
            if json {
                output::json(&serde_json::json!({
                    "defaultWorkspaceId": config.default_workspace_id,
                    "workspaces": session.workspaces,
                }))
            } else {
                output::workspaces(
                    &session.workspaces,
                    effective_saved_workspace(&config).as_deref(),
                );
                Ok(())
            }
        }
        WorkspaceCommand::Use { workspace } => {
            let selected = resolve_workspace(&session.workspaces, &workspace)?;
            if !selected.writable {
                return Err(CliError::InvalidInput(format!(
                    "workspace {:?} is read-only",
                    selected.name
                )));
            }
            config_store.save(&Config {
                default_workspace_id: Some(selected.id.clone()),
            })?;
            if json {
                output::json(selected)
            } else {
                println!(
                    "{} Default workspace: {}",
                    output::paint("1;32", "✓"),
                    selected.name
                );
                Ok(())
            }
        }
    }
}

async fn start(
    api: &ApiClient,
    args: StartArgs,
    json: bool,
    config_store: &ConfigStore,
) -> Result<()> {
    let session = api.session().await?;
    let config = config_store.load()?;
    let workspace = select_workspace(&session, args.workspace.as_deref(), &config)?;
    if !workspace.writable {
        return Err(CliError::InvalidInput(format!(
            "workspace {:?} is read-only",
            workspace.name
        )));
    }
    let resources = api.resources(&workspace.id).await?;
    let selection = resolve_start_selection(&args, &resources.projects, &resources.tags)?;
    for warning in &selection.warnings {
        eprintln!("Warning: {warning}");
    }
    let tag_implies_project = selection.tag_ids.iter().any(|id| {
        resources
            .tags
            .iter()
            .any(|tag| &tag.id == id && tag.project_id.is_some())
    });
    if workspace.kind == WorkspaceKind::Team
        && selection.project_id.is_none()
        && !tag_implies_project
    {
        return Err(CliError::InvalidInput(
            "team timers need a project: add @project to the task or use --project <name-or-id>"
                .into(),
        ));
    }
    let timer = api
        .start_timer(&StartTimerRequest {
            workspace_id: workspace.id.clone(),
            task_name: selection.task_name,
            project_id: selection.project_id,
            tag_ids: selection.tag_ids,
        })
        .await?;
    if json {
        output::json(&timer)
    } else {
        println!("{} Timer started!", output::paint("1;32", "⏱"));
        output::timer_status(&timer);
        Ok(())
    }
}

async fn stop(api: &ApiClient, json: bool) -> Result<()> {
    let timer = match api.stop_timer().await {
        Ok(timer) => timer,
        // The server answers 409 when nothing is running; stopping is idempotent.
        Err(CliError::Api { status, .. }) if status == StatusCode::CONFLICT => TimerResponse {
            running: false,
            entry: None,
            as_of: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            elapsed_ms: 0,
        },
        Err(error) => return Err(error),
    };
    if json {
        output::json(&timer)
    } else if timer.entry.is_some() {
        println!("{} Timer stopped — nice work!", output::paint("1;33", "✓"));
        Ok(())
    } else {
        println!("No timer running");
        Ok(())
    }
}

async fn status(api: &ApiClient, json: bool) -> Result<()> {
    let timer = api.timer().await?;
    if json {
        output::json(&timer)
    } else {
        output::timer_status(&timer);
        Ok(())
    }
}

async fn report(
    api: &ApiClient,
    args: &ReportArgs,
    json: bool,
    config_store: &ConfigStore,
) -> Result<()> {
    let session = api.session().await?;
    let config = config_store.load()?;
    let workspace = select_workspace(&session, args.workspace.as_deref(), &config)?;
    let (from, to) = report_dates(args, Local::now().date_naive())?;
    let scope = if args.all_members { "all" } else { "mine" };
    let mut report = api
        .report_page(
            &workspace.id,
            &from.to_string(),
            &to.to_string(),
            scope,
            None,
            if args.summary { 1 } else { 100 },
        )
        .await?;
    // Totals cover the whole range on every page, so a summary needs no entries.
    if args.summary {
        report.entries.clear();
        report.next_cursor = None;
    }
    let mut cursor = report.next_cursor.clone();
    while let Some(current) = cursor {
        let page = api
            .report_page(
                &workspace.id,
                &from.to_string(),
                &to.to_string(),
                scope,
                Some(&current),
                100,
            )
            .await?;
        if page.next_cursor.as_deref() == Some(current.as_str()) {
            return Err(CliError::Response(
                "report pagination returned a repeated cursor".into(),
            ));
        }
        report.entries.extend(page.entries);
        cursor = page.next_cursor;
        report.next_cursor.clone_from(&cursor);
    }
    if json {
        output::json(&report)
    } else {
        output::report(&report, args.summary);
        Ok(())
    }
}

fn effective_saved_workspace(config: &Config) -> Option<String> {
    env::var("TTRACKII_WORKSPACE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| config.default_workspace_id.clone())
}

fn select_workspace<'a>(
    session: &'a SessionResponse,
    explicit: Option<&str>,
    config: &Config,
) -> Result<&'a Workspace> {
    if let Some(identifier) = explicit {
        return resolve_workspace(&session.workspaces, identifier);
    }
    if let Ok(identifier) = env::var("TTRACKII_WORKSPACE") {
        if !identifier.trim().is_empty() {
            return resolve_workspace(&session.workspaces, &identifier);
        }
    }
    if let Some(identifier) = &config.default_workspace_id {
        if let Ok(workspace) = resolve_workspace(&session.workspaces, identifier) {
            return Ok(workspace);
        }
    }
    session
        .workspaces
        .iter()
        .find(|workspace| workspace.kind == WorkspaceKind::Personal)
        .or_else(|| session.workspaces.first())
        .ok_or_else(|| CliError::InvalidInput("no accessible workspaces".into()))
}

fn resolve_workspace<'a>(workspaces: &'a [Workspace], identifier: &str) -> Result<&'a Workspace> {
    if let Some(exact) = workspaces
        .iter()
        .find(|workspace| workspace.id == identifier)
    {
        return Ok(exact);
    }
    let needle = identifier.trim().to_lowercase();
    let matches = workspaces
        .iter()
        .filter(|workspace| {
            (needle == "personal" && workspace.kind == WorkspaceKind::Personal)
                || workspace.name.to_lowercase() == needle
                || workspace
                    .slug
                    .as_ref()
                    .is_some_and(|slug| slug.to_lowercase() == needle)
        })
        .collect::<Vec<_>>();
    unique_match(&matches, "workspace", identifier)
}

/// What `start` sends after reading flags and the task text.
#[derive(Debug, PartialEq, Eq)]
struct StartSelection {
    task_name: String,
    project_id: Option<String>,
    tag_ids: Vec<String>,
    /// Projects and tags that don't exist; the timer starts without them.
    warnings: Vec<String>,
}

/// Combines `--project`/`--tag` with `@project` and `#tag` in the task, as the web
/// timer does: `@project` is removed from the name, `#tag` stays in it (the web
/// shows it as a badge). Names that don't exist produce a warning and are skipped;
/// an unknown token stays in the name as typed.
fn resolve_start_selection(
    args: &StartArgs,
    projects: &[Resource],
    tags: &[Resource],
) -> Result<StartSelection> {
    let task = args.task.join(" ");
    let mut kept: Vec<String> = Vec::new();
    let mut mentioned_project: Option<&Resource> = None;
    let mut hashtag_ids = Vec::new();
    let mut removed_any = false;
    let mut warnings = Vec::new();
    for word in task.split_whitespace() {
        let mut chars = word.chars();
        let sigil = chars.next();
        let rest = chars.as_str();
        let token_len = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(rest.len());
        let (token, trailing) = rest.split_at(token_len);
        match sigil {
            Some('@') if !token.is_empty() => {
                if let Some(project) = match_token(projects, token, "project")? {
                    if mentioned_project.is_some_and(|other| other.id != project.id) {
                        return Err(CliError::InvalidInput(
                            "the task mentions more than one @project".into(),
                        ));
                    }
                    mentioned_project = Some(project);
                    removed_any = true;
                    // Keep punctuation after the token: "fix @site, then" -> "fix, then".
                    match kept.last_mut() {
                        Some(previous) if !trailing.is_empty() => previous.push_str(trailing),
                        _ if !trailing.is_empty() => kept.push(trailing.to_string()),
                        _ => {}
                    }
                    continue;
                }
                warnings.push(format!("no project named @{token}; starting without it"));
            }
            Some('#') if !token.is_empty() => {
                if let Some(tag) = match_token(tags, token, "tag")? {
                    hashtag_ids.push(tag.id.clone());
                } else {
                    warnings.push(format!("no tag named #{token}; starting without it"));
                }
            }
            _ => {}
        }
        kept.push(word.to_string());
    }
    let task_name = if removed_any {
        kept.join(" ")
    } else {
        task.trim().to_string()
    };

    let mut flag_project = None;
    if let Some(identifier) = args.project.as_deref() {
        flag_project = find_resource(projects, identifier, "project")?.map(|p| p.id.clone());
        if flag_project.is_none() {
            warnings.push(format!(
                "no project named {identifier:?}; starting without it"
            ));
        }
    }
    let project_id = match (flag_project, mentioned_project) {
        (Some(flag), Some(mentioned)) if flag != mentioned.id => {
            return Err(CliError::InvalidInput(format!(
                "--project and @{} in the task name point to different projects",
                token_key(&mentioned.name)
            )));
        }
        (flag, mentioned) => flag.or_else(|| mentioned.map(|project| project.id.clone())),
    };

    let mut tag_ids: Vec<String> = Vec::new();
    for identifier in &args.tag {
        match find_resource(tags, identifier, "tag")? {
            Some(tag) if !tag_ids.contains(&tag.id) => tag_ids.push(tag.id.clone()),
            Some(_) => {}
            None => warnings.push(format!("no tag named {identifier:?}; starting without it")),
        }
    }
    for id in hashtag_ids {
        if !tag_ids.contains(&id) {
            tag_ids.push(id);
        }
    }
    if let Some(project_id) = &project_id {
        for id in &tag_ids {
            let tag = tags.iter().find(|tag| &tag.id == id);
            if let Some(tag) =
                tag.filter(|tag| tag.project_id.as_ref().is_some_and(|p| p != project_id))
            {
                return Err(CliError::InvalidInput(format!(
                    "tag {:?} belongs to a different project",
                    tag.name
                )));
            }
        }
    }
    Ok(StartSelection {
        task_name,
        project_id,
        tag_ids,
        warnings,
    })
}

/// A `--project`/`--tag` value by ID or exact name (case-insensitive); `None` when
/// nothing matches, an error when the name is ambiguous.
fn find_resource<'a>(
    resources: &'a [Resource],
    identifier: &str,
    label: &str,
) -> Result<Option<&'a Resource>> {
    if let Some(exact) = resources.iter().find(|resource| resource.id == identifier) {
        return Ok(Some(exact));
    }
    let needle = identifier.trim().to_lowercase();
    let matches = resources
        .iter()
        .filter(|resource| resource.name.to_lowercase() == needle)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return Ok(None);
    }
    unique_match(&matches, label, identifier).map(Some)
}

/// How a name is written after `@` or `#`: spaces become hyphens, case is ignored.
fn token_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

/// The single resource a `@`/`#` token names, `None` when nothing matches (the
/// token is then ordinary text), or an error when the name is ambiguous.
fn match_token<'a>(
    resources: &'a [Resource],
    token: &str,
    label: &str,
) -> Result<Option<&'a Resource>> {
    let key = token.to_lowercase();
    let matches = resources
        .iter()
        .filter(|resource| token_key(&resource.name) == key)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [single] => Ok(Some(*single)),
        many => Err(CliError::InvalidInput(format!(
            "{token:?} matches multiple {label}s: {}",
            many.iter()
                .map(|item| item.identifier())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn unique_match<'a, T: Identified>(
    matches: &[&'a T],
    label: &str,
    identifier: &str,
) -> Result<&'a T> {
    match matches {
        [single] => Ok(*single),
        [] => Err(CliError::InvalidInput(format!(
            "no {label} matches {identifier:?}"
        ))),
        many => Err(CliError::InvalidInput(format!(
            "{identifier:?} matches multiple {label}s: {}",
            many.iter()
                .map(|item| item.identifier())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

trait Identified {
    fn identifier(&self) -> String;
}

impl Identified for Workspace {
    fn identifier(&self) -> String {
        format!("{} ({})", self.name, self.id)
    }
}

impl Identified for Resource {
    fn identifier(&self) -> String {
        format!("{} ({})", self.name, self.id)
    }
}

fn report_dates(args: &ReportArgs, today: NaiveDate) -> Result<(NaiveDate, NaiveDate)> {
    if let (Some(from), Some(to)) = (args.from, args.to) {
        if from > to {
            return Err(CliError::InvalidInput(
                "--from must not be later than --to".into(),
            ));
        }
        return Ok((from, to));
    }
    let period = args.period.unwrap_or(ReportPeriod::ThisWeek);
    let week_start = today
        .checked_sub_days(Days::new(u64::from(today.weekday().num_days_from_monday())))
        .expect("valid local date");
    let month_start = today.with_day(1).expect("day one is valid");
    let year_start = today.with_month(1).unwrap().with_day(1).unwrap();
    let range = match period {
        ReportPeriod::Today => (today, today),
        ReportPeriod::ThisWeek => (week_start, week_start + Days::new(6)),
        ReportPeriod::LastWeek => {
            let start = week_start - Days::new(7);
            (start, start + Days::new(6))
        }
        ReportPeriod::ThisMonth => (month_start, month_end(month_start)),
        ReportPeriod::LastMonth => {
            let start = month_start.checked_sub_months(Months::new(1)).unwrap();
            (start, month_end(start))
        }
        ReportPeriod::Quarter => {
            let quarter_month = ((today.month() - 1) / 3) * 3 + 1;
            let start = today
                .with_month(quarter_month)
                .unwrap()
                .with_day(1)
                .unwrap();
            let end = start
                .checked_add_months(Months::new(3))
                .unwrap()
                .checked_sub_days(Days::new(1))
                .unwrap();
            (start, end)
        }
        ReportPeriod::Year => {
            let end = year_start
                .checked_add_months(Months::new(12))
                .unwrap()
                .checked_sub_days(Days::new(1))
                .unwrap();
            (year_start, end)
        }
    };
    Ok(range)
}

fn month_end(month_start: NaiveDate) -> NaiveDate {
    month_start
        .checked_add_months(Months::new(1))
        .unwrap()
        .checked_sub_days(Days::new(1))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_args(period: ReportPeriod) -> ReportArgs {
        ReportArgs {
            workspace: None,
            period: Some(period),
            from: None,
            to: None,
            all_members: false,
            summary: false,
        }
    }

    #[test]
    fn report_periods_use_inclusive_boundaries() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 21).unwrap();
        assert_eq!(
            report_dates(&report_args(ReportPeriod::ThisWeek), today).unwrap(),
            (
                NaiveDate::from_ymd_opt(2026, 7, 20).unwrap(),
                NaiveDate::from_ymd_opt(2026, 7, 26).unwrap()
            )
        );
        assert_eq!(
            report_dates(&report_args(ReportPeriod::Quarter), today).unwrap(),
            (
                NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
            )
        );
    }

    #[test]
    fn detects_ambiguous_workspace_names() {
        let workspaces = vec![
            Workspace {
                id: "one".into(),
                kind: WorkspaceKind::Team,
                name: "Studio".into(),
                slug: None,
                role: None,
                writable: true,
            },
            Workspace {
                id: "two".into(),
                kind: WorkspaceKind::Team,
                name: "studio".into(),
                slug: None,
                role: None,
                writable: true,
            },
        ];
        let error = resolve_workspace(&workspaces, "STUDIO").unwrap_err();
        assert!(error.to_string().contains("one"));
        assert!(error.to_string().contains("two"));
    }

    #[tokio::test]
    async fn stop_without_running_timer_succeeds() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/api/v1/timer/stop"))
            .respond_with(
                ResponseTemplate::new(409).set_body_json(
                    serde_json::json!({ "message": "No timer is currently running" }),
                ),
            )
            .mount(&server)
            .await;
        let api = ApiClient::new(&server.uri(), Some("session".into())).unwrap();

        stop(&api, true).await.unwrap();
    }

    #[tokio::test]
    async fn stop_still_reports_other_api_errors() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/api/v1/timer/stop"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let api = ApiClient::new(&server.uri(), Some("session".into())).unwrap();

        assert!(matches!(
            stop(&api, true).await,
            Err(CliError::Api { status, .. }) if status == StatusCode::INTERNAL_SERVER_ERROR
        ));
    }

    fn resource(id: &str, name: &str, project_id: Option<&str>) -> Resource {
        Resource {
            id: id.into(),
            name: name.into(),
            project_id: project_id.map(Into::into),
        }
    }

    fn start_args(task: &str, project: Option<&str>, tags: &[&str]) -> StartArgs {
        StartArgs {
            task: task.split(' ').map(Into::into).collect(),
            project: project.map(Into::into),
            tag: tags.iter().map(|tag| (*tag).into()).collect(),
            workspace: None,
        }
    }

    fn fixtures() -> (Vec<Resource>, Vec<Resource>) {
        let projects = vec![
            resource("p-site", "Website", None),
            resource("p-client", "Client Work", None),
            resource("p-other", "Other", None),
        ];
        let tags = vec![
            resource("t-bug", "bug", None),
            resource("t-urgent", "Urgent", None),
            resource("t-client", "Ügyfél", None),
            resource("t-other", "infra", Some("p-other")),
        ];
        (projects, tags)
    }

    fn select(task: &str, project: Option<&str>, tags: &[&str]) -> Result<StartSelection> {
        let (projects, all_tags) = fixtures();
        resolve_start_selection(&start_args(task, project, tags), &projects, &all_tags)
    }

    #[test]
    fn task_tokens_pick_project_and_tags_like_the_web_timer() {
        assert_eq!(
            select("Fix login @website #bug #Urgent", None, &[]).unwrap(),
            StartSelection {
                task_name: "Fix login #bug #Urgent".into(),
                project_id: Some("p-site".into()),
                tag_ids: vec!["t-bug".into(), "t-urgent".into()],
                warnings: vec![],
            }
        );
        // Spaces in names are written as hyphens; non-ASCII names work too.
        let selection = select("Review @client-work #ügyfél", None, &[]).unwrap();
        assert_eq!(selection.project_id.as_deref(), Some("p-client"));
        assert_eq!(selection.tag_ids, vec!["t-client".to_string()]);
        assert_eq!(selection.task_name, "Review #ügyfél");
    }

    #[test]
    fn unknown_names_warn_and_start_without_them() {
        assert_eq!(
            select("Reply to @anna about #42  today", None, &[]).unwrap(),
            StartSelection {
                task_name: "Reply to @anna about #42  today".into(),
                project_id: None,
                tag_ids: vec![],
                warnings: vec![
                    "no project named @anna; starting without it".into(),
                    "no tag named #42; starting without it".into(),
                ],
            }
        );
        let flags = select("Ship #bug", Some("Missing"), &["nope", "urgent"]).unwrap();
        assert_eq!(flags.project_id, None);
        assert_eq!(
            flags.tag_ids,
            vec!["t-urgent".to_string(), "t-bug".to_string()]
        );
        assert_eq!(
            flags.warnings,
            vec![
                "no project named \"Missing\"; starting without it".to_string(),
                "no tag named \"nope\"; starting without it".to_string(),
            ]
        );
        assert_eq!(
            select("Deploy @website, then rest", None, &[])
                .unwrap()
                .task_name,
            "Deploy, then rest"
        );
        assert_eq!(
            select("email@website.com", None, &[]).unwrap().project_id,
            None
        );
    }

    #[test]
    fn flags_combine_with_tokens() {
        let selection = select("Triage #bug", Some("Website"), &["urgent", "bug"]).unwrap();
        assert_eq!(selection.project_id.as_deref(), Some("p-site"));
        assert_eq!(
            selection.tag_ids,
            vec!["t-urgent".to_string(), "t-bug".to_string()]
        );
        assert_eq!(
            select("Ship @website", Some("website"), &[])
                .unwrap()
                .project_id
                .as_deref(),
            Some("p-site")
        );
    }

    #[test]
    fn conflicting_projects_and_tags_are_refused() {
        assert!(select("Ship @website", Some("Other"), &[]).is_err());
        assert!(select("Ship @website @other", None, &[]).is_err());
        assert!(select("Ship @website #infra", None, &[]).is_err());
    }

    #[test]
    fn start_request_sends_ordered_tag_ids() {
        let body = |tag_ids: Vec<String>| {
            serde_json::to_value(StartTimerRequest {
                workspace_id: "personal".into(),
                task_name: "Fix #bug".into(),
                project_id: None,
                tag_ids,
            })
            .unwrap()
        };
        assert_eq!(
            body(vec!["t-urgent".into(), "t-bug".into()]),
            serde_json::json!({ "workspaceId": "personal", "taskName": "Fix #bug", "tagIds": ["t-urgent", "t-bug"] })
        );
        assert_eq!(
            body(vec![]),
            serde_json::json!({ "workspaceId": "personal", "taskName": "Fix #bug" })
        );
    }
}

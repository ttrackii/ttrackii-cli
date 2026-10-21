use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Local};
use serde::Serialize;

use crate::{
    error::{CliError, Result},
    model::{ReportEntry, ReportGroup, ReportResponse, SessionResponse, TimerResponse, Workspace},
};

static COLORS_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_colors_enabled(enabled: bool) {
    COLORS_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn paint(code: &str, value: impl std::fmt::Display) -> String {
    let value = value.to_string();
    if COLORS_ENABLED.load(Ordering::Relaxed) {
        format!("\x1b[{code}m{value}\x1b[0m")
    } else {
        value
    }
}

pub fn json<T: Serialize>(value: &T) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|error| CliError::Response(error.to_string()))?
    );
    Ok(())
}

pub fn session(session: &SessionResponse) {
    let name = session.user.name.as_deref().unwrap_or("Unnamed user");
    let email = session.user.email.as_deref().unwrap_or("no email");
    println!("{} <{}>", paint("1;36", name), email);
    println!("{} {}", paint("2", "User ID:"), session.user.id);
}

pub fn workspaces(workspaces: &[Workspace], selected_id: Option<&str>) {
    let rows: Vec<Vec<String>> = workspaces
        .iter()
        .map(|workspace| {
            vec![
                if selected_id == Some(workspace.id.as_str()) {
                    paint("1;32", "●")
                } else {
                    String::new()
                },
                workspace.name.clone(),
                workspace.kind.to_string(),
                workspace.role.clone().unwrap_or_else(|| "—".into()),
                workspace.id.clone(),
            ]
        })
        .collect();
    print_table(&["", "NAME", "KIND", "ROLE", "ID"], &rows);
}

pub fn timer_status(timer: &TimerResponse) {
    if !timer.running {
        println!("{} No timer running", paint("2", "○"));
        return;
    }
    let Some(entry) = &timer.entry else {
        println!(
            "{} Timer running — {}",
            paint("1;32", "●"),
            paint("1;33", format_duration(timer.elapsed_ms))
        );
        return;
    };
    println!(
        "{} {} — {}",
        paint("1;32", "●"),
        paint("1;36", &entry.task_name),
        paint("1;33", format_duration(timer.elapsed_ms))
    );
    println!("{} {}", paint("2", "Workspace:"), entry.workspace_name);
    if let Some(project) = &entry.project_name {
        println!("{} {project}", paint("2", "Project:"));
    }
    match entry.tag_names.as_slice() {
        [] => {
            if let Some(tag) = &entry.tag_name {
                println!("{} {tag}", paint("2", "Tag:"));
            }
        }
        [tag] => println!("{} {tag}", paint("2", "Tag:")),
        tags => println!("{} {}", paint("2", "Tags:"), tags.join(", ")),
    }
    println!(
        "{} {}",
        paint("2", "Started:"),
        format_timestamp(entry.start_time)
    );
}

pub fn report(report: &ReportResponse, summary_only: bool) {
    println!(
        "{} {} · {} to {}",
        paint("1;36", "▸"),
        report.workspace.name,
        report.start_date,
        report.end_date
    );
    println!(
        "{} {} · {} entries{} · Billable {}",
        paint("1;36", "✦"),
        paint("1;33", format_duration(report.summary.total_duration_ms)),
        report.summary.entry_count,
        if report.summary.running_entry_count > 0 {
            format!(", {} running", report.summary.running_entry_count)
        } else {
            String::new()
        },
        format_billable_totals(report)
    );
    print_groups("Projects", &report.summary.by_project);
    print_groups("Tags", &report.summary.by_tag);
    if !report.summary.by_member.is_empty() {
        println!("\nMembers");
        let rows = report
            .summary
            .by_member
            .iter()
            .map(|member| {
                vec![
                    member
                        .name
                        .clone()
                        .or_else(|| member.email.clone())
                        .unwrap_or_else(|| member.user_id.clone()),
                    format_duration(member.duration_ms),
                ]
            })
            .collect::<Vec<_>>();
        print_table(&["NAME", "TOTAL"], &rows);
    }

    if summary_only {
        return;
    }
    if report.entries.is_empty() {
        println!("\n{} No entries in this period", paint("2", "✦"));
        return;
    }
    println!("\n{} Entries", paint("1;36", "✦"));
    let show_member = report
        .entries
        .iter()
        .any(|entry| entry.member_name.is_some());
    let mut headers = vec!["DATE", "TASK", "PROJECT", "DURATION"];
    if show_member {
        headers.insert(1, "MEMBER");
    }
    let rows: Vec<Vec<String>> = report
        .entries
        .iter()
        .map(|entry| entry_row(entry, show_member))
        .collect();
    print_table(&headers, &rows);
}

fn entry_row(entry: &ReportEntry, show_member: bool) -> Vec<String> {
    let mut row = vec![
        format_timestamp(entry.start_time)
            .chars()
            .take(10)
            .collect(),
    ];
    if show_member {
        row.push(entry.member_name.clone().unwrap_or_else(|| "—".into()));
    }
    row.extend([
        entry.task_name.clone(),
        entry.project_name.clone().unwrap_or_else(|| "—".into()),
        if entry.is_running {
            format!("{} (running)", format_duration(entry.duration_ms))
        } else {
            format_duration(entry.duration_ms)
        },
    ]);
    row
}

fn print_groups(label: &str, groups: &[ReportGroup]) {
    if groups.is_empty() {
        return;
    }
    println!("\n{label}");
    let rows = groups
        .iter()
        .map(|group| vec![group.name.clone(), format_duration(group.duration_ms)])
        .collect::<Vec<_>>();
    print_table(&["NAME", "TOTAL"], &rows);
}

fn format_billable_totals(report: &ReportResponse) -> String {
    if report.summary.billable_totals.is_empty() {
        return "—".into();
    }
    report
        .summary
        .billable_totals
        .iter()
        .map(|total| {
            let absolute = total.amount_minor.unsigned_abs();
            let sign = if total.amount_minor < 0 { "-" } else { "" };
            format!(
                "{} {sign}{}.{:02}",
                total.currency.to_uppercase(),
                absolute / 100,
                absolute % 100,
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_timestamp(epoch_milliseconds: i64) -> String {
    DateTime::from_timestamp_millis(epoch_milliseconds).map_or_else(
        || epoch_milliseconds.to_string(),
        |timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        },
    )
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let widths = (0..headers.len())
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(String::len)
                .chain(std::iter::once(headers[column].len()))
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let mut line = String::new();
    for (column, header) in headers.iter().enumerate() {
        if column > 0 {
            line.push_str("  ");
        }
        let _ = write!(line, "{header:<width$}", width = widths[column]);
    }
    println!("{}", line.trim_end());
    for row in rows {
        line.clear();
        for (column, cell) in row.iter().enumerate() {
            if column > 0 {
                line.push_str("  ");
            }
            let _ = write!(line, "{cell:<width$}", width = widths[column]);
        }
        println!("{}", line.trim_end());
    }
}

#[must_use]
pub fn format_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_elapsed_duration() {
        assert_eq!(format_duration(999), "0s");
        assert_eq!(format_duration(65_000), "1m 05s");
        assert_eq!(format_duration(3_900_000), "1h 05m");
    }
}

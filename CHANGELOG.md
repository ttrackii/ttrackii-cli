# Changelog

## 0.1.0

First public release.

- `ttrackii login` signs in through your browser with a one-time device code (`--no-open` prints the link instead). The session is stored only in the operating system's credential store: macOS Keychain, Windows Credential Manager, or Linux Secret Service.
- `ttrackii logout` revokes the session and removes the stored credential.
- `ttrackii whoami` shows the signed-in account.
- `ttrackii workspace list` and `ttrackii workspace use <workspace>` list workspaces and set the default one.
- `ttrackii start`, `ttrackii stop`, and `ttrackii status` control the timer, with optional `--project`, `--tag`, and `--workspace`. Starting a timer stops the one already running.
- `ttrackii report` shows totals, per-project and per-tag breakdowns, and entries for `today`, `this-week` (the default), `last-week`, `this-month`, `last-month`, `quarter`, `year`, or a `--from`/`--to` range. `--summary` leaves out the entry list; `--all-members` includes the whole team for workspace admins.
- Every command accepts `--json` for stable machine-readable output. `--no-color` and `NO_COLOR` turn off color.
- `TTRACKII_TOKEN` accepts an API key or CLI session for scripts and CI.

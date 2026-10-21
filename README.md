# ttrackii CLI

The official cross-platform command-line client for [ttrackii](https://ttrackii.com).

## Install

Prebuilt packages are published for Windows, macOS, and Linux with each GitHub
release.

On macOS or Linux, install through the Homebrew tap:

```shell
brew install ttrackii/tap/ttrackii
```

Or use the install script:

```shell
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ttrackii/ttrackii-cli/releases/latest/download/ttrackii-cli-installer.sh | sh
```

On Windows, run the PowerShell installer, or download the `.msi` from the
[latest release](https://github.com/ttrackii/ttrackii-cli/releases/latest):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/ttrackii/ttrackii-cli/releases/latest/download/ttrackii-cli-installer.ps1 | iex"
```

Windows builds are not code-signed yet, so SmartScreen may ask you to confirm
the installer.

You can also compile and install from crates.io on any supported platform:

```shell
cargo install ttrackii-cli
```

The installed binary is named `ttrackii`.

## Authenticate

```shell
ttrackii login
```

The CLI opens ttrackii in your browser and waits for approval. First enable
**Settings → API & CLI → Allow API access** in the web app, then approve the
device code.
It stores the resulting session only in the operating system's secure
credential store: macOS Keychain, Windows Credential Manager, or Linux Secret
Service. It never falls back to a plaintext token file. Use `--no-open` on a
headless machine.

For CI, provide an API key (or a CLI device session) through `TTRACKII_TOKEN`
from Settings → API & CLI. Environment credentials take precedence over the secure
credential store and are never printed in human or JSON output.

The API accepts only CLI sessions created by `ttrackii login` and API keys
(`tt_live_…`); browser and mobile-app sessions are rejected. The HTTP surface is
`https://app.ttrackii.com/api/v1`.

## Usage

```shell
# Account and workspaces
ttrackii whoami
ttrackii workspace list
ttrackii workspace use personal
ttrackii workspace use my-team

# Timers
ttrackii start "Client kickoff"
ttrackii start "Fix login @website #bug #urgent"   # @project and #tags, as in the web app
ttrackii start "Landing page" --workspace my-team --project Website --tag design
ttrackii status
ttrackii stop

# Reports (the current Monday–Sunday week is the default)
ttrackii report
ttrackii report --period today
ttrackii report --period last-month
ttrackii report --from 2026-07-01 --to 2026-07-31
ttrackii report --workspace my-team --all-members
ttrackii report --period last-month --summary   # totals only, no entry list
```

In the task, `@name` picks a project and is removed from the task name; `#name`
links a tag and stays in it, as in the web app. Write spaces in names as
hyphens (`@client-work`); case doesn't matter, and tokens that match nothing stay
plain text. Quote the task when it contains `#`, because bash treats an unquoted
`#word` as a comment.

Every authenticated command accepts `--json` for machine-readable output.
`--no-color` and the standard `NO_COLOR` environment variable disable color.
Human-readable output uses light ANSI color and small status icons; JSON output
never contains color codes and remains safe to pipe.

Workspace selection uses the following precedence:

1. `--workspace`
2. `TTRACKII_WORKSPACE`
3. the value saved by `workspace use`
4. the personal workspace

`TTRACKII_API_URL` overrides the production API origin for development. Remote
origins must use HTTPS; plain HTTP is accepted only for `localhost`,
`127.0.0.1`, and `::1`.

## Security

Please report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Development

```shell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

This project is licensed under the [MIT License](LICENSE).

# Releasing ttrackii CLI

Releases are built by cargo-dist 0.32.0. A version tag creates the GitHub release, platform archives, SHA-256 checksums, shell and PowerShell installers, a Windows MSI, Linux packages, a Homebrew formula, and the crates.io package, plus `THIRD_PARTY_LICENSES.md` in every archive.

Not enabled yet: Windows code signing and WinGet. Both are configured but switched off in `dist-workspace.toml`; see [Enabling Windows signing](#enabling-windows-signing) and [Enabling WinGet](#enabling-winget).

## One-time repository setup

Create `ttrackii/homebrew-tap`, initialize it with a default branch, and grant the release repository write access. Configure these GitHub Actions secrets:

- `CARGO_REGISTRY_TOKEN`: crates.io token allowed to publish `ttrackii-cli`.
- `HOMEBREW_TAP_TOKEN`: fine-grained GitHub token with contents write access to `ttrackii/homebrew-tap`.

The repository must be public before the first release: GitHub artifact attestations need a public repository on the free plan, and Homebrew and the install scripts download release assets anonymously.

## Enabling Windows signing

Add `ssldotcom-windows-sign = "prod"` to `[dist]` in `dist-workspace.toml`, run `dist generate`, and configure `SSLDOTCOM_USERNAME`, `SSLDOTCOM_PASSWORD`, `SSLDOTCOM_TOTP_SECRET`, and `SSLDOTCOM_CREDENTIAL_ID` (SSL.com eSigner production credentials) as Actions secrets. Then remove the SmartScreen note from the README.

## Enabling WinGet

Add `post-announce-jobs = ["./publish-winget"]` and `"publish-winget" = { contents = "read" }` (in `github-custom-job-permissions`) to `dist-workspace.toml` and run `dist generate`. WinGet also needs this one-time setup:

1. Submit and merge the first `ttrackii.ttrackii` manifest manually with Microsoft's WingetCreate tool. The automated `update` command intentionally requires an existing community manifest to use as its trusted metadata base.
2. Use a dedicated GitHub bot account that has accepted Microsoft's Contributor License Agreement and can create a fork and pull requests against `microsoft/winget-pkgs`.
3. Create a protected GitHub Actions environment named `winget`. Add an environment secret named `WINGET_TOKEN` containing a short-lived classic PAT with only the `public_repo` scope; WingetCreate does not currently support fine-grained PATs. Do not grant `delete_repo` or full `repo`. Add required reviewers to the environment if WinGet submissions should have a manual approval gate.
4. Rotate the PAT on an explicit schedule. The workflow maps it only to WingetCreate's recommended `WINGET_CREATE_GITHUB_TOKEN` environment variable and never passes it on the command line.

The WinGet job pins Microsoft's WingetCreate 1.12.8.0 executable and verifies SHA-256 `8bd738851b524885410112678e3771b341c5c716de60fbbecb88ab0a363ed85d` before use. Update both values together after verifying a newer official release.

## Cut a release

1. Update the version in `Cargo.toml` and run `cargo update -w` so `Cargo.lock` carries the same package version.
2. Add the version's section to `CHANGELOG.md` (cargo-dist uses it as the release notes), regenerate third-party notices with `cargo about generate --fail -o THIRD_PARTY_LICENSES.md about.hbs`, then run `cargo fmt --all --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --all-features`, and `cargo package --locked`.
3. Verify the generated workflow is current with `dist generate --check` using cargo-dist 0.32.0.
4. Commit the release changes and push a tag named `v<version>`, for example `v0.1.0`.
5. Confirm the GitHub Release contains archives for macOS Intel/Apple Silicon, Linux x86-64/ARM64, and Windows x86-64; `.deb` and `.rpm` packages for both Linux architectures; the MSI; checksums; installers; and `dist-manifest.json`.
6. Smoke-test `ttrackii --version` from one artifact on each operating system and verify the Homebrew tap and crates.io version.
7. Once WinGet is enabled: confirm the release workflow opened a WinGet PR for `ttrackii.ttrackii`. After Microsoft merges it and the community source refreshes, verify on Windows with `winget show --exact --id ttrackii.ttrackii --versions`, then install the released version with `winget install --exact --id ttrackii.ttrackii --version <version>`.

Prerelease tags build GitHub artifacts but are not published to Homebrew, crates.io, or WinGet. WinGet publication means submitting a community-repository pull request; availability is delayed until Microsoft validation and review complete. The workflow is idempotent for versions already merged or represented by an open PR. Keep `dist-workspace.toml` as the release source of truth; after changing it, run `dist generate` and commit the regenerated workflow and WiX template.

macOS archives are covered by GitHub artifact attestations. Developer ID signing and notarization need Apple credentials and a dedicated signing step before the archives are made public; do not claim notarized distribution until that step and its secrets are configured.

## Pinned GitHub Actions

Every third-party action is pinned to a commit SHA with its version in a trailing comment. The release workflow's pins live in `[dist.github-action-commits]` in `dist-workspace.toml`; when Dependabot proposes a new SHA for `release.yml`, copy it into that table and run `dist generate` instead of merging the workflow edit directly. Pins in the other workflows can be merged from Dependabot as usual.

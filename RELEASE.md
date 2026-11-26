# Releasing TimberStat

Versions follow [Semantic Versioning](https://semver.org/) and are computed from
[Conventional Commits](https://www.conventionalcommits.org/) by git-cliff (`cliff.toml`):

| Commit type | Before 1.0.0 | From 1.0.0 |
|---|---|---|
| `fix:` | patch (0.1.0 → 0.1.1) | patch |
| `feat:` | minor (0.1.0 → 0.2.0) | minor |
| breaking (`feat!:` or `BREAKING CHANGE:`) | minor | major |

Other types (`docs`, `test`, `chore`, `ci`, `refactor`, `perf`) appear in the changelog;
`perf` and `refactor` do not bump the version on their own.

## Beta builds (automatic)

Every push to `main` runs CI. When CI passes, `.github/workflows/beta.yml` builds the
packages and replaces the rolling **beta** pre-release on GitHub. File names carry the next
version plus `-beta.<run>` (for example `TimberStat-0.2.0-beta.14-windows-x64-setup.exe`).
Beta builds never create a version tag.

## Official releases (when you push a tag)

`.github/workflows/release.yml` runs when a tag like `v0.2.0` is pushed. It checks that the
tag matches the workspace version in `Cargo.toml`, runs CI, builds, and publishes a GitHub
release with notes from git-cliff and `SHA256SUMS.txt`. Nothing is published if CI or the
Windows build fails. The Linux build is best effort: when it fails, the release ships
Windows files only.

One-time tool install:

```sh
cargo install --locked cargo-release git-cliff
```

Cut a release from a clean `main`:

```sh
# Next version from the commits since the last tag, e.g. 0.2.0
next=$(git cliff --bumped-version | sed 's/^v//')
# Bumps Cargo.toml, regenerates CHANGELOG.md, commits "chore(release): v$next" and tags v$next
cargo release "$next" --execute
# Publishing starts here
git push origin main --follow-tags
```

### The first release (0.1.0)

The workspace is already at 0.1.0, so there is nothing to bump. Tag it directly:

```sh
git cliff --tag v0.1.0 --output CHANGELOG.md
git commit -am "chore(release): v0.1.0"
git tag -a v0.1.0 -m "TimberStat v0.1.0"
git push origin main --follow-tags
```

## Artifacts

| Platform | File | Notes |
|---|---|---|
| Windows x64 | `TimberStat-<version>-windows-x64-setup.exe` | NSIS, per-user install, no admin prompt, uninstaller |
| Windows x64 | `TimberStat-<version>-windows-x64.msi` | WiX, per-machine install (needs admin) |
| Windows x64 | `TimberStat-<version>-windows-x64-portable.zip` | `timberstat.exe`, no install |
| Linux x64 | `TimberStat-<version>-linux-x64.deb` | best effort |
| Linux x64 | `TimberStat-<version>-linux-x64.AppImage` | best effort |
| Linux x64 | `TimberStat-<version>-linux-x64-portable.tar.gz` | best effort |
| All | `SHA256SUMS.txt` | checksums of every file above |

Builds are **not code-signed**. On first run Windows SmartScreen shows "Windows protected your
PC"; choose **More info**, then **Run anyway**. Settings and logs live in
`%APPDATA%\TimberStat` and `%LOCALAPPDATA%\TimberStat` for every Windows build, including the
portable ZIP.

## Packaging locally

```sh
cargo install --locked cargo-packager
cd crates/app
cargo packager --release -f nsis,wix     # Windows
cargo packager --release -f deb,appimage # Linux
```

Output goes to `target/release`. Packaging settings are under
`[package.metadata.packager]` in `crates/app/Cargo.toml`. App icons are not set yet; add
them there (`icons = [...]`) once the logo exists.

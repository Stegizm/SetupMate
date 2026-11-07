# AGENTS.md

## Project

SetupMate is a safety-first, GUI-based helper for new Linux users. It detects the distro family, GPU vendor, laptop/desktop form factor, and Wayland/X11 session, then suggests catalog-backed packages. It is a guide, not a one-click installer.

## Non-negotiable rules

- Never invent Linux package names. Names come only from `data/catalog/packages.toml`.
- Unknown, ambiguous, distro-specific, driver-related, or repo/AUR/RPM Fusion/PPA/Flatpak-remote names must be `TODO_VERIFY`.
- Never generate an install command for `TODO_VERIFY`.
- Default to a preview plan. Install only after explicit user approval (the GUI approval dialog is that gate).
- v0.1 must not: remove packages, add repos, import signing keys, edit bootloader/fstab/systemd, load or change kernel modules, overwrite configs, or modify dotfiles.
- GPU driver packages are high-risk: guide-only in v0.1 as well.
- Keep core logic independent of the GUI.
- Library code must not panic; use `Result`. Avoid `unwrap`/`expect` outside tests.
- Do not add dependencies without user approval.
- Code, identifiers, file names, and commands stay English; user-facing explanations are Turkish.

## Structure

- `crates/setupmate-core` — GUI-independent library:
  - `models` — data types (distro, GPU, form factor, session, risk, verification).
  - `detection` — read-only probes (`/etc/os-release`, `XDG_SESSION_TYPE`, sysfs). No external commands. `gpu_vendors: Vec<GpuVendor>` lists every GPU (hybrid laptops show both). `distro_family_from_os_release` maps base distros **and derivatives** from `ID`/`ID_LIKE` (Mint/Pop/Kali→Debian, Nobara/Alma/Rocky→Fedora, Manjaro/CachyOS/EndeavourOS→Arch, Leap/Tumbleweed→openSUSE, postmarketOS→Alpine).
  - `installed` — best-effort read-only probe of the local package DB: Debian `dpkg` status file, Arch pacman `local/`, Alpine `/lib/apk/db/installed` (`P:` lines), Void `pkgdb-*.plist` (top-level XML keys), and Fedora/openSUSE via `rusqlite` reading `rpmdb.sqlite`'s `Name` index (read-only). When no DB can be found/parsed the snapshot is unreliable and every lookup is `Unknown`; never guesses.
  - `catalog` — TOML loading + `Catalog::validate` consistency checks. The three TOML files are also compiled into the binary with `include_str!` (`Catalog::load_embedded`); `SETUPMATE_CATALOG_DIR` is a development override.
  - `package_managers` — distro→manager mapping and command construction (no execution): apt/dnf/pacman/zypper/apk/xbps. `non_interactive_command` adds the manager's confirmation flag for the live path (`-y`, `--noconfirm`, `--non-interactive`, `--no-interactive`).
  - `planner` — preview plans; excludes `TODO_VERIFY` and high-risk entries. `live_plan` rewrites an approved plan into non-interactive commands and drops anything that cannot be mapped.
  - `executor` — `CommandRunner` trait, dry-run default, explicit `Approval` gate, `SystemRunner` (runs through `pkexec` when not root), and `execute_plan_with_progress` (stop at first failure, callback per entry).
  - `reporting` — Turkish plan/report rendering.
  - `sha256` — dependency-free SHA-256 used for update verification.
  - `sys` — PATH lookup, root check, `pkexec` elevation prefix.
  - `updater` — GitHub release self-update for the AppImage (`releases/latest/download/SHA256SUMS`), downloads via `curl`/`wget`, verifies, replaces `$APPIMAGE`. AUR/package-manager installs are not self-updated.
- `crates/setupmate-gui` — eframe/egui 0.37 dashboard UI. Shows detection, catalog status, selection and plan; live install only after the confirmation dialog. Also provides CLI modes `--version`, `--help`, `--check-update`, `--update`.
- `data/catalog/{categories,packages,profiles}.toml` — catalog data. `packages.toml` holds verified entries for all six families (debian/fedora/arch/opensuse/alpine/void); names are added only once verified against official sources.
- `packaging/` — `build-appimage.sh`, desktop entry, SVG icon, and AUR source `PKGBUILD` + `.SRCINFO`.
- `.github/workflows/` — `ci.yml` (fmt + clippy + test) and `release.yml` (tag-triggered AppImage + `SHA256SUMS` release).

## Commands

Rust is installed via rustup; in a fresh shell source the environment first:

```bash
. "$HOME/.cargo/env"
cargo check --workspace
cargo test --workspace
cargo run -p setupmate-gui
```

Run a single test with `cargo test -p setupmate-core <test_name>`. Quality gates (same as CI in `.github/workflows/ci.yml`): `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`.

Packaging: `./packaging/build-appimage.sh` writes `dist/SetupMate-<version>-<arch>.AppImage` + `SHA256SUMS`; `packaging/aur/PKGBUILD` is the source package. A `v*` tag triggers `.github/workflows/release.yml`, which must keep the asset name `SetupMate-<version>-<arch>.AppImage` because the updater derives the version from it.

## GUI notes (eframe 0.37)

- `eframe::App` uses `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)`, not `update(ctx, frame)`.
- eframe 0.37 unifies side/top panels into `Panel`: use `Panel::top("id")`, `Panel::left("id")` (and `CentralPanel`). All are shown inside a `Ui`: `.show(ui, ...)`. `SidePanel`/`TopBottomPanel` no longer exist in 0.37.
- The GUI uses the `glow` renderer with `default-features = false` (wgpu disabled); keep this in mind before changing eframe features.
- Styling lives in `setupmate-gui/src/main.rs` (`configure_style` + the `card`/`badge`/`primary_button` helpers). The palette is a professional dark theme — deep navy canvas, navy-teal surfaces, hairline borders, restrained shadows and a single teal accent (dark text on teal for contrast). `configure_style` forces `ThemePreference::Dark`. Layout: top bar, left sidebar (system/profiles/catalog), central work area (categories/packages/plan).
- Buttons with an explicit `.fill(...)` ignore hover/active states in 0.37; `primary_button` overrides the widget visuals inside a `ui.scope` so the teal button still reacts to hover.
- The logo is vector-drawn in `setupmate-gui/src/main.rs` (`paint_logo`): a teal rounded tile with a dark "tune" (three sliders) glyph. It needs no asset file and no image-decoding crate; swap `paint_logo` if a different mark is wanted later.
- The window icon is generated in code (`make_icon`) by rasterising the same mark into RGBA and passing it via `ViewportBuilder::with_icon`. No image crate or asset file.
- **Live install:** the plan card's "Kurulumu başlat" button only opens an `egui::Window` confirmation dialog listing the exact non-interactive commands (`planner::live_plan`). "Onayla ve kur" starts a background thread with `SystemRunner::elevated()` + `execute_plan_with_progress` (`ExecutionMode::Apply`, `Approval::explicit()`); progress entries arrive over an `mpsc` channel and are drained by `poll_install`. The run stops at the first failure, the log is shown in the plan card, and `installed` is re-detected afterwards so badges refresh. `pkexec` missing → the UI shows the Turkish error and points at the copyable plan. After a run the plan section also shows a low/medium/high risk-count summary.
- The layout is responsive: below 980px available width (`narrow`) the left panel shrinks (304→264), inner margins tighten, and the stats strip falls back to 2 cards per row via `ui.columns`. TODO_VERIFY catalogue entries stay visible in the package list with a `TODO_VERIFY` status badge and are listed under "Plandan çıkarılanlar" with their Turkish reason.
- The central area starts with a "Rehber" onboarding card: a 4-step tracker (system → purpose → tick packages → approve & install) whose step markers turn green as the user progresses. Below it a "Eksik önerilenler" card lists verified packages that `installed` reports as `Missing`, grouped by category with per-category counts; when the DB cannot be read it shows an explicit "unknown" note instead of guessing. Package rows also carry a `kurulu`/`eksik` badge when the install status is known.
- **Package selection:** the app holds `checked: HashSet<String>` of catalog ids and the dry-run plan is built **only** from those ticked ids. Each package row has a checkbox (disabled for TODO_VERIFY rows, which can never reach the plan). A "Görünümü seç / Görünümü temizle" button bulk-ticks/clears the visible verified rows.
- A **"Seçim" summary strip** (below the stats) shows the ticked packages as removable chips, a global "Seçimi temizle" button, and one-click profile quick sets ("Dizüstü için öner") that tick every verified package in that profile's categories.
- **Plan export:** the plan card has "Planı kopyala" (clipboard via `ui.ctx().copy_text`) and "Planı kaydet" (writes `setupmate-plan.txt` under the XDG data dir). Both build plain text via `plan_export_text`; nothing is executed and no config is changed. Source URLs in package rows are clickable `ui.hyperlink_to` links.
- **List controls:** the package card has a sort combo (Varsayılan / Ada göre / Riske göre / Kurulu duruma göre) and a "Yalnızca düşük risk" checkbox. Sorting/filtering is view-only; it never changes `checked`.
- The category chip row has three scopes that are mutually exclusive: "Tümü", "Önerilen" (matched hardware profiles) and "Eksikler · N" (only not-yet-installed verified packages, shown when N>0). Clicking any scope chip **pre-ticks** that view's verified ids into `checked` (via `view_ids`), so beginners get a safe starting set they can then untick; manual ticks add/remove on top. A friendly hint below the chips explains the active scope (the category description when a single category is selected).
- Categories with **zero** packages for the detected family are hidden (`build_categories` filters them), so Alpine/Void users do not see e.g. "Oyun · 0". The catalog has 13 categories; gaming entries exist only for the four desktop families.
- The stats strip is a dynamic card list: Kategoriler, Paket kayıtları, Eksik (only when the DB is readable), Seçili (ticked count) and Plan, laid out `narrow ? 2 : all` per row.
- GPU detection is multi-vendor: `SystemInfo.gpu_vendors: Vec<GpuVendor>` holds every distinct vendor in `/sys/class/drm` (hybrid laptops list both iGPU and dGPU). `Catalog::matching_profiles` unions all matching GPU profiles; the GUI shows them joined (e.g. "Intel + NVIDIA"). Do not collapse this back to a single first-match vendor.

## Conventions

- Keep changes small and compiling; generally at most 3 files per step.
- Record critical decisions in `docs/ai/decisions.md`.
- Track unverified package names in `docs/ai/todo-package-names.md`.

## opencode.json

Defines the `tokenharbor` provider and `deepseek-v4.1-flash:free` model. It contains a plaintext `apiKey`: never print, echo, copy, or commit it. It is listed in `.gitignore`; keep it out of the repository.
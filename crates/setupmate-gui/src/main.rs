//! SetupMate GUI — professional dashboard with an explicitly approved live
//! install path.
//!
//! Layout: a top bar, a left sidebar (system detection, profiles, catalog
//! status) and a central work area (categories, packages, plan).
//! The plan is always shown as a preview first; commands only run after the
//! user confirms the separate approval dialog.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use eframe::egui::{
    self, Align, CentralPanel, Color32, CornerRadius, FontId, Frame, Layout, Margin, Panel,
    RichText, ScrollArea, Sense, Shadow, Stroke, TextStyle, Vec2,
};
use setupmate_core::catalog::{Catalog, PackageEntry};
use setupmate_core::detection;
use setupmate_core::executor::{self, Approval, ExecutionMode, ExecutionReport, SystemRunner};
use setupmate_core::installed::{InstallStatus, InstalledPackages};
use setupmate_core::models::{
    DistroFamily, FormFactor, GpuVendor, PackageRef, PackageSuggestion, RiskLevel, SessionType,
    SystemInfo, VerificationStatus,
};
use setupmate_core::planner;
use setupmate_core::updater;

/// Current application version, shown in the UI and used for update checks.
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Professional dark palette: deep navy canvas, navy-teal surfaces, teal accent.
const BG: Color32 = Color32::from_rgb(0x0B, 0x12, 0x20);
const SURFACE: Color32 = Color32::from_rgb(0x12, 0x1C, 0x2E);
const SURFACE_ALT: Color32 = Color32::from_rgb(0x0E, 0x17, 0x26);
const BORDER: Color32 = Color32::from_rgb(0x24, 0x32, 0x49);
const TEXT: Color32 = Color32::from_rgb(0xE7, 0xEE, 0xF9);
const MUTED: Color32 = Color32::from_rgb(0x93, 0xA4, 0xBC);
const ACCENT: Color32 = Color32::from_rgb(0x2D, 0xD4, 0xBF);
const ACCENT_HOVER: Color32 = Color32::from_rgb(0x5E, 0xEA, 0xD4);
const ON_ACCENT: Color32 = Color32::from_rgb(0x04, 0x23, 0x1F);
const OK: Color32 = Color32::from_rgb(0x34, 0xD3, 0x99);
const OK_BG: Color32 = Color32::from_rgb(0x0E, 0x2A, 0x24);
const WARN: Color32 = Color32::from_rgb(0xFB, 0xBF, 0x24);
const WARN_BG: Color32 = Color32::from_rgb(0x2A, 0x22, 0x10);
const DANGER: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);
const DANGER_BG: Color32 = Color32::from_rgb(0x2A, 0x14, 0x16);

fn main() -> eframe::Result<()> {
    if let Some(code) = run_cli() {
        std::process::exit(code);
    }
    run_gui()
}

fn run_gui() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 840.0])
            .with_min_inner_size([860.0, 620.0])
            .with_icon(make_icon(128))
            .with_title("SetupMate"),
        ..Default::default()
    };

    eframe::run_native(
        "SetupMate",
        options,
        Box::new(|cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(SetupMateApp::new()))
        }),
    )
}

/// Handles terminal-only modes. Returns `None` when the GUI should start.
fn run_cli() -> Option<i32> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let first = arguments.first()?.as_str();

    match first {
        "--version" | "-V" => {
            println!("SetupMate {VERSION}");
            Some(0)
        }
        "--help" | "-h" => {
            print_help();
            Some(0)
        }
        "--check-update" => Some(check_update_cli()),
        "--update" => Some(update_cli()),
        _ => None,
    }
}

fn print_help() {
    println!(
        "SetupMate {VERSION}
Güvenli Linux kurulum rehberi.

Kullanım:
  setupmate                 Grafik arayüzü açar.
  setupmate --check-update  Yeni sürüm olup olmadığını denetler.
  setupmate --update        AppImage kurulumunu son sürüme günceller.
  setupmate --version       Sürümü yazar.
  setupmate --help          Bu yardımı gösterir.

Not: AppImage dışındaki (AUR/paket yöneticisi) kurulumlar kendini güncellemez;
dağıtımının paket yöneticisiyle güncelle (ör. `yay -Syu`)."
    );
}

fn check_update_cli() -> i32 {
    let repo = updater::repo_slug();
    match updater::check_for_update(&repo, VERSION, std::env::consts::ARCH) {
        Ok(Some(info)) => {
            println!(
                "Yeni sürüm var: {} (kurulu: {}).",
                info.latest, info.current
            );
            println!("Güncellemek için: setupmate --update");
            0
        }
        Ok(None) => {
            println!("En güncel sürümdesin ({VERSION}).");
            0
        }
        Err(error) => {
            eprintln!("Güncelleme denetlenemedi: {error}");
            1
        }
    }
}

fn update_cli() -> i32 {
    let repo = updater::repo_slug();
    match updater::update(&repo, VERSION, std::env::consts::ARCH, None) {
        Ok(updater::UpdateOutcome::UpToDate { current }) => {
            println!("Zaten güncel: {current}.");
            0
        }
        Ok(updater::UpdateOutcome::Updated { from, to, path }) => {
            println!("Güncellendi: {from} -> {to}");
            println!("Yeni AppImage: {}", path.display());
            println!("Yeniden başlatınca yeni sürüm açılır.");
            0
        }
        Err(error) => {
            eprintln!("Güncelleme başarısız: {error}");
            1
        }
    }
}

/// Rasterises the vector mark into an RGBA window icon.
///
/// Mirrors `paint_logo`: a teal rounded tile with three dark "tune" sliders.
/// Generated in code so no image-decoding crate or asset file is needed.
fn make_icon(size: u32) -> egui::IconData {
    let s = size as f32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let radius = s * 0.24;
    let rows = [(0.34_f32, 0.62_f32), (0.5, 0.32), (0.66, 0.74)];
    let left = 0.26 * s;
    let right = 0.74 * s;
    let line_half = 0.055 * s;
    let knob_radius = 0.085 * s;

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            if !inside_rounded(px, py, s, radius) {
                continue;
            }

            let mut color = ACCENT;
            for (row, position) in rows {
                let cy = row * s;
                if px >= left && px <= right && (py - cy).abs() <= line_half {
                    color = ON_ACCENT;
                }
                let cx = left + (right - left) * position;
                if ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() <= knob_radius {
                    color = ON_ACCENT;
                }
            }

            let index = ((y * size + x) * 4) as usize;
            rgba[index] = color.r();
            rgba[index + 1] = color.g();
            rgba[index + 2] = color.b();
            rgba[index + 3] = 255;
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn inside_rounded(px: f32, py: f32, size: f32, radius: f32) -> bool {
    let min = radius;
    let max = size - radius;
    let cx = px.clamp(min, max);
    let cy = py.clamp(min, max);
    (px - cx).powi(2) + (py - cy).powi(2) <= radius * radius
}

fn configure_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = SURFACE;
        style.visuals.extreme_bg_color = SURFACE_ALT;
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.selection.stroke = Stroke::new(1.0, ON_ACCENT);
        style.visuals.window_corner_radius = CornerRadius::same(10);

        let widgets = &mut style.visuals.widgets;
        widgets.noninteractive.bg_fill = SURFACE_ALT;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
        widgets.noninteractive.corner_radius = CornerRadius::same(8);

        widgets.inactive.weak_bg_fill = SURFACE;
        widgets.inactive.bg_fill = SURFACE;
        widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        widgets.inactive.corner_radius = CornerRadius::same(8);

        widgets.hovered.weak_bg_fill = SURFACE_ALT;
        widgets.hovered.bg_fill = SURFACE_ALT;
        widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        widgets.hovered.fg_stroke = Stroke::new(1.0, ACCENT_HOVER);
        widgets.hovered.corner_radius = CornerRadius::same(8);

        widgets.active.weak_bg_fill = SURFACE_ALT;
        widgets.active.bg_fill = SURFACE_ALT;
        widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
        widgets.active.fg_stroke = Stroke::new(1.0, ACCENT_HOVER);
        widgets.active.corner_radius = CornerRadius::same(8);

        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.window_margin = Margin::same(12);

        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(20.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(13.0));
    });
}

/// A restrained drop-shadow used by cards and the top bar.
fn soft_shadow() -> Shadow {
    Shadow {
        offset: [0, 2],
        blur: 8,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(0, 0, 0, 110),
    }
}

#[derive(Clone)]
struct CategoryView {
    id: String,
    name: String,
    description: String,
    package_count: usize,
}

#[derive(Clone)]
struct ProfileView {
    name: String,
    description: String,
    category_ids: Vec<String>,
}

#[derive(Clone)]
struct PackageView {
    catalog_id: String,
    category_id: String,
    package: String,
    reason: String,
    source: String,
    risk: RiskLevel,
    status: VerificationStatus,
    install: InstallStatus,
}

struct CatalogInfo {
    loaded: bool,
    error: Option<String>,
    category_count: usize,
    profile_count: usize,
    package_count: usize,
    validation_errors: Vec<String>,
}

/// One line in the live-install progress log.
#[derive(Clone)]
struct InstallLogLine {
    catalog_id: String,
    command: String,
    ok: bool,
    message: String,
}

/// State of a live install run, updated from the background thread.
#[derive(Clone)]
struct InstallRun {
    total: usize,
    done: usize,
    log: Vec<InstallLogLine>,
    report: Option<ExecutionReport>,
    error: Option<String>,
    finished: bool,
}

impl InstallRun {
    fn starting(total: usize) -> Self {
        Self {
            total,
            done: 0,
            log: Vec::new(),
            report: None,
            error: None,
            finished: false,
        }
    }

    fn failed(message: String) -> Self {
        Self {
            total: 0,
            done: 0,
            log: Vec::new(),
            report: None,
            error: Some(message),
            finished: true,
        }
    }

    fn running(&self) -> bool {
        !self.finished
    }
}

/// Messages sent from the install thread back to the UI.
enum InstallMessage {
    Started { total: usize },
    Entry(InstallLogLine),
    Finished { report: ExecutionReport },
}

/// What the approval dialog decided this frame.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfirmAction {
    None,
    Cancel,
    Install,
}

struct SetupMateApp {
    system: SystemInfo,
    installed: InstalledPackages,
    catalog: Result<Catalog, String>,
    selected_category: Option<String>,
    use_recommended: bool,
    missing_only: bool,
    /// Catalog ids the user ticked; the plan is built only from these.
    checked: HashSet<String>,
    search: String,
    /// Package list ordering.
    sort: PackageSort,
    /// When set, the package list only shows low-risk entries.
    low_risk_only: bool,
    /// Feedback line after a plan export/copy action.
    export_note: Option<String>,
    /// Whether the explicit install-confirmation dialog is open.
    confirm_install: bool,
    /// Live install progress, present while and after a run.
    install: Option<InstallRun>,
    /// Channel from the background install thread; `None` when idle.
    install_rx: Option<Receiver<InstallMessage>>,
}

/// How the package list is ordered.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PackageSort {
    Catalog,
    Name,
    Risk,
    Install,
}

impl PackageSort {
    fn label(self) -> &'static str {
        match self {
            PackageSort::Catalog => "Varsayılan",
            PackageSort::Name => "Ada göre",
            PackageSort::Risk => "Riske göre",
            PackageSort::Install => "Kurulu duruma göre",
        }
    }
}

impl SetupMateApp {
    fn new() -> Self {
        let system = detection::detect();
        let installed = InstalledPackages::detect(system.distro_family);
        Self {
            system,
            installed,
            catalog: load_catalog(),
            selected_category: None,
            use_recommended: false,
            missing_only: false,
            checked: HashSet::new(),
            search: String::new(),
            sort: PackageSort::Catalog,
            low_risk_only: false,
            export_note: None,
            confirm_install: false,
            install: None,
            install_rx: None,
        }
    }

    fn refresh(&mut self) {
        self.system = detection::detect();
        self.installed = InstalledPackages::detect(self.system.distro_family);
        self.catalog = load_catalog();
        self.selected_category = None;
        self.use_recommended = false;
        self.missing_only = false;
        self.checked.clear();
        self.search.clear();
        self.low_risk_only = false;
        self.export_note = None;
        self.confirm_install = false;
        self.install = None;
        self.install_rx = None;
    }

    fn install_running(&self) -> bool {
        self.install_rx.is_some() || self.install.as_ref().is_some_and(InstallRun::running)
    }

    /// Drains progress messages from the install thread.
    fn poll_install(&mut self, ctx: &egui::Context) {
        let mut messages = Vec::new();
        let mut disconnected = false;

        if let Some(rx) = &self.install_rx {
            loop {
                match rx.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        let mut finished = false;
        for message in messages {
            match message {
                InstallMessage::Started { total } => {
                    if let Some(run) = &mut self.install {
                        run.total = total;
                    }
                }
                InstallMessage::Entry(line) => {
                    if let Some(run) = &mut self.install {
                        run.done += 1;
                        run.log.push(line);
                    }
                }
                InstallMessage::Finished { report } => {
                    if let Some(run) = &mut self.install {
                        run.finished = true;
                        run.report = Some(report);
                    }
                    finished = true;
                }
            }
        }

        if disconnected && !finished {
            if let Some(run) = &mut self.install {
                run.finished = true;
                if run.error.is_none() {
                    run.error =
                        Some("Kurulum iş parçacığı beklenmedik şekilde kapandı.".to_string());
                }
            }
            finished = true;
        }

        if finished {
            self.install_rx = None;
            // Reflect the new package state in badges and the missing list.
            self.installed = InstalledPackages::detect(self.system.distro_family);
            ctx.request_repaint();
        }
    }

    /// Starts an approved live install on a background thread.
    fn start_install(&mut self, live: planner::InstallPlan, ctx: egui::Context) {
        if self.install_running() {
            return;
        }

        let total = live.actions.len();
        if total == 0 {
            self.install = Some(InstallRun::failed(
                "Kurulacak doğrulanmış paket yok.".to_string(),
            ));
            return;
        }

        let runner = match SystemRunner::elevated() {
            Ok(runner) => runner,
            Err(error) => {
                self.install = Some(InstallRun::failed(format!(
                    "{error}\n\nPlanı kopyalayıp terminalde `sudo` ile çalıştırabilirsin."
                )));
                return;
            }
        };

        let (tx, rx) = mpsc::channel();
        self.install = Some(InstallRun::starting(total));
        self.install_rx = Some(rx);

        thread::spawn(move || {
            let mut runner = runner;
            let _ = tx.send(InstallMessage::Started { total });

            let mut report = executor::execute_plan_with_progress(
                &live,
                &mut runner,
                ExecutionMode::Apply,
                Some(Approval::explicit()),
                |entry| {
                    let line = InstallLogLine {
                        catalog_id: entry.catalog_id.clone(),
                        command: entry.command.join(" "),
                        ok: entry.error.is_none(),
                        message: entry
                            .error
                            .clone()
                            .unwrap_or_else(|| "tamamlandı".to_string()),
                    };
                    let _ = tx.send(InstallMessage::Entry(line));
                    ctx.request_repaint();
                },
            );

            // The UI only needs success/error per entry; drop potentially large
            // stdout/stderr before sending the report across the channel.
            for entry in &mut report.entries {
                entry.output = None;
            }

            let _ = tx.send(InstallMessage::Finished { report });
            ctx.request_repaint();
        });
    }
}

fn load_catalog() -> Result<Catalog, String> {
    // Development override kept for editing the catalog without rebuilding.
    if let Some(dir) = std::env::var_os("SETUPMATE_CATALOG_DIR") {
        if !dir.is_empty() {
            return Catalog::load_dir(PathBuf::from(dir)).map_err(|error| error.to_string());
        }
    }

    Catalog::load_embedded().map_err(|error| error.to_string())
}

impl eframe::App for SetupMateApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_install(ui.ctx());
        let ctx = ui.ctx().clone();
        let narrow = ui.available_width() < 980.0;
        let mut refresh = false;
        let mut selected = self.selected_category.clone();
        let mut use_recommended = self.use_recommended;
        let mut missing_only = self.missing_only;
        let mut checked = self.checked.clone();
        let mut scope_changed = false;
        let mut search = self.search.clone();
        let mut sort = self.sort;
        let mut low_risk_only = self.low_risk_only;
        let mut export_note = self.export_note.clone();
        let mut confirm_requested = false;
        let system = self.system.clone();

        // Snapshot everything the UI needs as owned data so the closures below
        // never borrow `self` while `selected`/`refresh` are mutated.
        let info = build_catalog_info(&self.catalog);
        let catalog = self.catalog.as_ref().ok();
        let family = system.distro_family;
        let packages = build_packages(catalog, family, &self.installed);
        let install_known = self.installed.is_reliable();
        let missing_count = packages
            .iter()
            .filter(|package| {
                package.status == VerificationStatus::Verified
                    && package.install == InstallStatus::Missing
            })
            .count();
        let categories = build_categories(catalog, &packages);
        let matched_profiles = catalog
            .map(|catalog| catalog.matching_profiles(&system))
            .unwrap_or_default();
        let profiles: Vec<ProfileView> = matched_profiles
            .iter()
            .map(|profile| ProfileView {
                name: profile.name.clone(),
                description: profile.description.clone(),
                category_ids: profile.category_ids.clone(),
            })
            .collect();
        let recommended_ids: Vec<String> = {
            let mut ids = Vec::new();
            for profile in &matched_profiles {
                for id in &profile.category_ids {
                    if !ids.contains(id) {
                        ids.push(id.clone());
                    }
                }
            }
            ids
        };
        let recommended_count = packages
            .iter()
            .filter(|package| recommended_ids.contains(&package.category_id))
            .count();
        let active: Option<Vec<String>> = if use_recommended {
            Some(recommended_ids.clone())
        } else {
            selected.clone().map(|id| vec![id])
        };
        // The plan is built only from the packages the user ticked.
        let all_suggestions = filtered_suggestions(catalog, family, None, false, &self.installed);
        let suggestions: Vec<PackageSuggestion> = all_suggestions
            .into_iter()
            .filter(|suggestion| checked.contains(&suggestion.catalog_id))
            .collect();
        let plan = planner::build_plan(&system, &suggestions);
        // The approved run uses the non-interactive variant of the same plan.
        let live_plan = planner::live_plan(&plan);
        let install_snapshot = self.install.clone();
        let install_finished = install_snapshot.as_ref().is_some_and(|run| run.finished);
        let scope_label = if missing_only {
            "Eksik önerilenler".to_string()
        } else if use_recommended {
            "Önerilen (profilden)".to_string()
        } else if let Some(id) = &selected {
            categories
                .iter()
                .find(|category| &category.id == id)
                .map(|category| category.name.clone())
                .unwrap_or_else(|| "Seçili kategori".to_string())
        } else {
            "Tüm kategoriler".to_string()
        };

        Panel::top("setupmate_top")
            .frame(
                Frame::NONE
                    .fill(SURFACE)
                    .inner_margin(Margin::symmetric(20, 12)),
            )
            .show(ui, |ui| {
                top_bar(ui, &mut refresh);
            });

        Panel::left("setupmate_side")
            .resizable(false)
            .exact_size(if narrow { 264.0 } else { 304.0 })
            .frame(
                Frame::NONE
                    .fill(SURFACE_ALT)
                    .inner_margin(Margin::same(if narrow { 12 } else { 16 })),
            )
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt("side_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        system_section(ui, &system);
                        ui.add_space(12.0);
                        profiles_section(ui, &profiles);
                        ui.add_space(12.0);
                        catalog_section(ui, &info);
                        ui.add_space(12.0);
                        safety_section(ui);
                    });
            });

        CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(BG)
                    .inner_margin(Margin::same(if narrow { 14 } else { 20 })),
            )
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt("main_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        guide_section(
                            ui,
                            &system,
                            selected.is_some() || use_recommended || missing_only,
                            checked.len(),
                            install_finished,
                        );
                        ui.add_space(16.0);
                        let mut stat_cards: Vec<(&str, String, &str)> = vec![
                            ("Kategoriler", info.category_count.to_string(), "katalog"),
                            ("Paket kayıtları", packages.len().to_string(), "bu dağıtım"),
                        ];
                        if install_known {
                            stat_cards.push(("Eksik", missing_count.to_string(), "kurulu değil"));
                        }
                        stat_cards.push(("Seçili", checked.len().to_string(), "işaretli paket"));
                        stat_cards.push((
                            "Plan",
                            format!("{} / {}", plan.actions.len(), plan.skipped.len()),
                            "adım / atlanan",
                        ));
                        stats_section(ui, &stat_cards, narrow);
                        ui.add_space(16.0);
                        selection_section(ui, &packages, &profiles, &mut checked);
                        ui.add_space(16.0);
                        missing_section(ui, &packages, &categories, install_known);
                        ui.add_space(16.0);
                        categories_section(
                            ui,
                            &categories,
                            &recommended_ids,
                            recommended_count,
                            missing_count,
                            &mut selected,
                            &mut use_recommended,
                            &mut missing_only,
                            &mut scope_changed,
                        );
                        ui.add_space(16.0);
                        packages_section(
                            ui,
                            &packages,
                            &scope_label,
                            active.as_deref(),
                            missing_only,
                            &mut checked,
                            &mut search,
                            &mut sort,
                            &mut low_risk_only,
                        );
                        ui.add_space(16.0);
                        plan_section(
                            ui,
                            &plan,
                            &system,
                            install_snapshot.as_ref(),
                            &mut confirm_requested,
                            &mut export_note,
                        );
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(
                                "SetupMate bir rehberdir; tek tuşla her şeyi kuran bir araç değildir.",
                            )
                            .size(11.5)
                            .color(MUTED),
                        );
                    });
            });

        // The install button only opens the dialog; nothing runs until the user
        // confirms inside it.
        if confirm_requested {
            self.confirm_install = true;
        }

        let action = if self.confirm_install {
            confirm_modal(&ctx, &live_plan)
        } else {
            ConfirmAction::None
        };

        match action {
            ConfirmAction::Install => {
                self.confirm_install = false;
                self.start_install(live_plan.clone(), ctx.clone());
            }
            ConfirmAction::Cancel => self.confirm_install = false,
            ConfirmAction::None => {}
        }

        // A new scope (Tümü/Önerilen/Eksikler/kategori) pre-ticks that view, so
        // beginners get a safe starting set they can then untick.
        if scope_changed {
            let new_active: Option<Vec<String>> = if use_recommended {
                Some(recommended_ids.clone())
            } else {
                selected.clone().map(|id| vec![id])
            };
            checked = view_ids(&packages, new_active.as_deref(), missing_only)
                .into_iter()
                .collect();
        }

        self.selected_category = selected;
        self.use_recommended = use_recommended;
        self.missing_only = missing_only;
        self.checked = checked;
        self.search = search;
        self.sort = sort;
        self.low_risk_only = low_risk_only;
        self.export_note = export_note;
        if refresh && !self.install_running() {
            self.refresh();
        }
    }
}

// ---------------------------------------------------------------------------
// Data building
// ---------------------------------------------------------------------------

fn build_catalog_info(catalog: &Result<Catalog, String>) -> CatalogInfo {
    match catalog {
        Ok(catalog) => {
            let validation_errors = match catalog.validate() {
                Ok(()) => Vec::new(),
                Err(errors) => errors.iter().map(|error| error.to_string()).collect(),
            };

            CatalogInfo {
                loaded: true,
                error: None,
                category_count: catalog.categories.categories.len(),
                profile_count: catalog.profiles.profiles.len(),
                package_count: catalog.packages.packages.len(),
                validation_errors,
            }
        }
        Err(error) => CatalogInfo {
            loaded: false,
            error: Some(error.clone()),
            category_count: 0,
            profile_count: 0,
            package_count: 0,
            validation_errors: Vec::new(),
        },
    }
}

fn build_packages(
    catalog: Option<&Catalog>,
    family: DistroFamily,
    installed: &InstalledPackages,
) -> Vec<PackageView> {
    catalog
        .map(|catalog| {
            catalog
                .packages
                .packages
                .iter()
                .filter(|entry| family == DistroFamily::Unknown || entry.family == family)
                .map(|entry| {
                    let suggestion = entry.to_suggestion();
                    let package_name = match &suggestion.package {
                        PackageRef::Name(name) => name.clone(),
                        PackageRef::TodoVerify => "TODO_VERIFY".to_string(),
                    };
                    let install = if suggestion.verification_status == VerificationStatus::Verified
                    {
                        installed.status(&package_name)
                    } else {
                        InstallStatus::Unknown
                    };
                    PackageView {
                        catalog_id: entry.catalog_id.clone(),
                        category_id: entry.category_id.clone(),
                        package: package_name,
                        reason: entry.reason.clone(),
                        source: entry.source.clone(),
                        risk: suggestion.risk,
                        status: suggestion.verification_status,
                        install,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Suggestions for the detected family, optionally limited to a category scope
/// and/or to packages that are not installed yet.
fn filtered_suggestions(
    catalog: Option<&Catalog>,
    family: DistroFamily,
    active: Option<&[String]>,
    missing_only: bool,
    installed: &InstalledPackages,
) -> Vec<PackageSuggestion> {
    catalog
        .map(|catalog| {
            catalog
                .packages
                .packages
                .iter()
                .filter(|entry| family == DistroFamily::Unknown || entry.family == family)
                .filter(|entry| {
                    active.is_none_or(|ids| ids.iter().any(|id| id == &entry.category_id))
                })
                .filter(|entry| !missing_only || entry_is_missing(entry, installed))
                .map(|entry| entry.to_suggestion())
                .collect()
        })
        .unwrap_or_default()
}

/// A catalog entry counts as "missing" when it is verified and its package name
/// is not found in the local package database.
fn entry_is_missing(entry: &PackageEntry, installed: &InstalledPackages) -> bool {
    entry.verification_status == VerificationStatus::Verified
        && installed.status(&entry.package) == InstallStatus::Missing
}

/// Catalog ids that belong to a scope and can actually be installed: verified
/// entries only, optionally limited to packages that are not installed yet.
/// Used to pre-tick a scope; users can still tick unverified rows by hand.
fn view_ids(
    packages: &[PackageView],
    active: Option<&[String]>,
    missing_only: bool,
) -> Vec<String> {
    packages
        .iter()
        .filter(|package| package.status == VerificationStatus::Verified)
        .filter(|package| active.is_none_or(|ids| ids.iter().any(|id| id == &package.category_id)))
        .filter(|package| !missing_only || package.install == InstallStatus::Missing)
        .map(|package| package.catalog_id.clone())
        .collect()
}

fn build_categories(catalog: Option<&Catalog>, packages: &[PackageView]) -> Vec<CategoryView> {
    catalog
        .map(|catalog| {
            catalog
                .categories
                .categories
                .iter()
                .filter_map(|category| {
                    let package_count = packages
                        .iter()
                        .filter(|package| package.category_id == category.id)
                        .count();
                    // Families differ: e.g. Alpine/Void have no gaming entries.
                    // Hide empty categories instead of showing "Oyun · 0".
                    (package_count > 0).then(|| CategoryView {
                        id: category.id.clone(),
                        name: category.name.clone(),
                        description: category.description.clone(),
                        package_count,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Shared widgets
// ---------------------------------------------------------------------------

/// A white surface card with a hairline border and a restrained shadow.
fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::NONE
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .shadow(soft_shadow())
        .show(ui, add_contents)
        .inner
}

/// A small colored pill (status/risk badge).
fn badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    Frame::NONE
        .fill(bg)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).strong().color(fg));
        });
}

fn empty_state(ui: &mut egui::Ui, text: &str) {
    Frame::NONE
        .fill(SURFACE_ALT)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(MUTED));
        });
}

fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(15.0).strong().color(TEXT));
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).size(12.0).color(MUTED));
    }
}

/// A label on the left and a right-aligned value on the same row.
fn kv_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.5).color(MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(13.0).strong().color(TEXT));
        });
    });
}

fn stat_card(ui: &mut egui::Ui, title: &str, value: &str, hint: &str) {
    card(ui, |ui| {
        ui.label(RichText::new(title).size(11.5).color(MUTED));
        ui.add_space(2.0);
        ui.label(RichText::new(value).size(22.0).strong().color(TEXT));
        ui.label(RichText::new(hint).size(10.5).color(MUTED));
    });
}

fn stats_section(ui: &mut egui::Ui, cards: &[(&str, String, &str)], narrow: bool) {
    let per_row = if narrow { 2 } else { cards.len().max(1) };
    for chunk in cards.chunks(per_row) {
        ui.columns(chunk.len(), |columns| {
            for (slot, (title, value, hint)) in chunk.iter().enumerate() {
                stat_card(&mut columns[slot], title, value.as_str(), hint);
            }
        });
        ui.add_space(8.0);
    }
}

/// Compact "what did I tick" strip: a global clear button, one-click profile
/// quick sets ("Dizüstü için öner") and a removable chip per selected package.
fn selection_section(
    ui: &mut egui::Ui,
    packages: &[PackageView],
    profiles: &[ProfileView],
    checked: &mut HashSet<String>,
) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Seçim").size(15.0).strong().color(TEXT));
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("{} paket işaretli", checked.len()))
                    .size(12.0)
                    .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let can_clear = !checked.is_empty();
                let button =
                    egui::Button::new(RichText::new("Seçimi temizle").size(12.0).color(TEXT));
                if ui.add_enabled(can_clear, button).clicked() {
                    checked.clear();
                }
            });
        });

        if !profiles.is_empty() {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Hazır set:").size(11.5).color(MUTED));
                for profile in profiles {
                    if ui
                        .small_button(format!("{} için öner", profile.name))
                        .on_hover_text(&profile.description)
                        .clicked()
                    {
                        for package in packages {
                            if package.status == VerificationStatus::Verified
                                && profile.category_ids.contains(&package.category_id)
                            {
                                checked.insert(package.catalog_id.clone());
                            }
                        }
                    }
                }
            });
        }

        ui.add_space(10.0);
        let selected: Vec<&PackageView> = packages
            .iter()
            .filter(|package| checked.contains(&package.catalog_id))
            .collect();
        if selected.is_empty() {
            empty_state(
                ui,
                "Henüz paket işaretlenmedi. Aşağıdan tik at ya da bir kapsam/profil seç.",
            );
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for package in selected {
                let label = RichText::new(format!("{}  ✕", package.package)).size(11.5);
                if ui
                    .small_button(label)
                    .on_hover_text(format!("{} — kaldır", package.catalog_id))
                    .clicked()
                {
                    checked.remove(&package.catalog_id);
                }
            }
        });
    });
}

fn guidance(ui: &mut egui::Ui) {
    Frame::NONE
        .fill(SURFACE_ALT)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.label(
                RichText::new("Nasıl paket eklenir?")
                    .size(12.5)
                    .strong()
                    .color(TEXT),
            );
            ui.add_space(4.0);
            for line in [
                "1. Paket adını dağıtımın resmi belgelerinden doğrula.",
                "2. Kaydı data/catalog/packages.toml içine source alanıyla ekle.",
                "3. \"Yeniden tespit et\" ile kataloğu yenile.",
            ] {
                ui.label(RichText::new(line).size(11.5).color(MUTED));
            }
        });
}

// ---------------------------------------------------------------------------
// Top bar / sidebar sections
// ---------------------------------------------------------------------------

/// A teal primary button with working hover/active feedback.
fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;

        widgets.inactive.weak_bg_fill = ACCENT;
        widgets.inactive.bg_fill = ACCENT;
        widgets.inactive.bg_stroke = Stroke::NONE;
        widgets.inactive.fg_stroke = Stroke::new(1.0, ON_ACCENT);

        widgets.hovered.weak_bg_fill = ACCENT_HOVER;
        widgets.hovered.bg_fill = ACCENT_HOVER;
        widgets.hovered.bg_stroke = Stroke::NONE;
        widgets.hovered.fg_stroke = Stroke::new(1.0, ON_ACCENT);

        widgets.active.weak_bg_fill = ACCENT_HOVER;
        widgets.active.bg_fill = ACCENT_HOVER;
        widgets.active.bg_stroke = Stroke::NONE;
        widgets.active.fg_stroke = Stroke::new(1.0, ON_ACCENT);

        ui.add(
            egui::Button::new(RichText::new(text).size(13.0).strong())
                .corner_radius(CornerRadius::same(8))
                .min_size(Vec2::new(158.0, 32.0)),
        )
    })
    .inner
}

/// A simple vector mark for the top bar: a teal tile with dark "tune" sliders.
fn paint_logo(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(rect, CornerRadius::same(10), ACCENT);

    let scale = size / 42.0;
    let left = rect.left() + 11.0 * scale;
    let right = rect.right() - 11.0 * scale;
    let line = Stroke::new(2.6 * scale, ON_ACCENT);
    let knob = 3.2 * scale;
    let rows = [(14.5_f32, 0.62_f32), (21.0, 0.32), (27.5, 0.74)];

    for (row, position) in rows {
        let cy = rect.top() + row * scale;
        painter.line_segment([egui::pos2(left, cy), egui::pos2(right, cy)], line);

        let cx = left + (right - left) * position;
        // Accent ring first, then the dark knob, so the knob reads clearly.
        painter.circle_filled(egui::pos2(cx, cy), knob + 1.6 * scale, ACCENT);
        painter.circle_filled(egui::pos2(cx, cy), knob, ON_ACCENT);
    }
}

fn top_bar(ui: &mut egui::Ui, refresh: &mut bool) {
    ui.horizontal(|ui| {
        paint_logo(ui, 42.0);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("SetupMate").size(17.0).strong().color(TEXT));
            ui.label(
                RichText::new("Güvenli Linux kurulum rehberi")
                    .size(12.0)
                    .color(MUTED),
            );
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if primary_button(ui, "Yeniden tespit et").clicked() {
                *refresh = true;
            }

            ui.add_space(6.0);
            badge(ui, "CANLI KURULUM", OK, OK_BG);
            ui.add_space(6.0);
            badge(ui, &format!("v{VERSION}"), MUTED, SURFACE_ALT);
        });
    });
}

fn system_section(ui: &mut egui::Ui, system: &SystemInfo) {
    card(ui, |ui| {
        section_title(ui, "Sistem", "Salt-okunur tespit");
        ui.add_space(10.0);
        kv_row(ui, "Dağıtım ailesi", distro_label(system.distro_family));
        kv_row(ui, "GPU üreticisi", &gpu_labels(&system.gpu_vendors));
        kv_row(ui, "Cihaz", form_factor_label(system.form_factor));
        kv_row(ui, "Oturum", session_label(system.session_type));
    });
}

fn profiles_section(ui: &mut egui::Ui, profiles: &[ProfileView]) {
    card(ui, |ui| {
        section_title(ui, "Profiller", "Donanıma göre eşleşen");
        ui.add_space(10.0);

        if profiles.is_empty() {
            empty_state(ui, "Eşleşen profil yok.");
            return;
        }

        for profile in profiles {
            ui.label(RichText::new(&profile.name).size(13.0).strong().color(TEXT));
            ui.label(RichText::new(&profile.description).size(11.5).color(MUTED));
            if !profile.category_ids.is_empty() {
                ui.label(
                    RichText::new(format!("Kategoriler: {}", profile.category_ids.join(", ")))
                        .size(11.0)
                        .color(MUTED),
                );
            }
            ui.add_space(8.0);
        }
    });
}

fn catalog_section(ui: &mut egui::Ui, info: &CatalogInfo) {
    card(ui, |ui| {
        section_title(ui, "Katalog", "Veri dosyaları");
        ui.add_space(10.0);

        if let Some(error) = &info.error {
            ui.label(
                RichText::new(format!("Katalog yüklenemedi: {error}"))
                    .size(11.5)
                    .color(DANGER),
            );
            return;
        }

        if !info.loaded {
            empty_state(ui, "Katalog durumu bilinmiyor.");
            return;
        }

        kv_row(ui, "Kategoriler", &info.category_count.to_string());
        kv_row(ui, "Profiller", &info.profile_count.to_string());
        kv_row(ui, "Paket kayıtları", &info.package_count.to_string());

        ui.add_space(8.0);
        if info.validation_errors.is_empty() {
            badge(ui, "Tutarlı", OK, OK_BG);
        } else {
            ui.horizontal(|ui| {
                badge(
                    ui,
                    &format!("{} sorun", info.validation_errors.len()),
                    DANGER,
                    DANGER_BG,
                );
                ui.label(RichText::new("doğrulama hatası").size(11.5).color(MUTED));
            });
            ui.add_space(4.0);
            for error in info.validation_errors.iter().take(3) {
                ui.label(RichText::new(format!("• {error}")).size(11.0).color(DANGER));
            }
        }

        if info.package_count == 0 {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Doğrulanmış paket yok; katalog verisini kontrol et.")
                    .size(11.0)
                    .color(MUTED),
            );
        }
    });
}

// ---------------------------------------------------------------------------
// Central sections
// ---------------------------------------------------------------------------

fn safety_section(ui: &mut egui::Ui) {
    card(ui, |ui| {
        section_title(ui, "Güvenlik", "Kurulum kuralları");
        ui.add_space(8.0);
        for line in [
            "Salt-okunur tespit",
            "Varsayılan: dry-run önizleme",
            "Kurulum yalnızca açık onayla çalışır",
            "Repo, anahtar ve bootloader değişmez",
            "GPU sürücüleri yalnızca rehber",
            "Güncelleme (AppImage): setupmate --update",
        ] {
            ui.label(RichText::new(format!("• {line}")).size(11.5).color(MUTED));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn categories_section(
    ui: &mut egui::Ui,
    categories: &[CategoryView],
    recommended_ids: &[String],
    recommended_count: usize,
    missing_count: usize,
    selected: &mut Option<String>,
    use_recommended: &mut bool,
    missing_only: &mut bool,
    scope_changed: &mut bool,
) {
    card(ui, |ui| {
        section_title(
            ui,
            "Kategoriler",
            "Kullanım amacına göre seçin; seçim paket listesini ve planı birlikte filtreler.",
        );
        ui.add_space(10.0);

        if categories.is_empty() {
            empty_state(ui, "Kategori bulunamadı.");
            return;
        }

        let chip = |label: String, active: bool| -> egui::Button<'static> {
            let (fill, text_color, border) = if active {
                (ACCENT, ON_ACCENT, ACCENT)
            } else {
                (SURFACE_ALT, TEXT, BORDER)
            };
            egui::Button::new(RichText::new(label).size(12.5).strong().color(text_color))
                .fill(fill)
                .corner_radius(CornerRadius::same(8))
                .stroke(Stroke::new(1.0, border))
                .min_size(Vec2::new(0.0, 30.0))
        };

        ui.horizontal_wrapped(|ui| {
            // "Tümü" — clears the category, recommended and missing scopes.
            let all_selected = selected.is_none() && !*use_recommended && !*missing_only;
            if ui.add(chip("Tümü".to_string(), all_selected)).clicked() {
                *selected = None;
                *use_recommended = false;
                *missing_only = false;
                *scope_changed = true;
            }

            // "Önerilen" — union of the matched hardware profiles' categories.
            if !recommended_ids.is_empty() {
                let button = chip(
                    format!("Önerilen  ·  {recommended_count}"),
                    *use_recommended,
                );
                if ui
                    .add(button)
                    .on_hover_text("Donanım profilinize göre güvenli başlangıç seti.")
                    .clicked()
                {
                    *selected = None;
                    *use_recommended = true;
                    *missing_only = false;
                    *scope_changed = true;
                }
            }

            // "Eksikler" — only the recommended packages not installed yet.
            if missing_count > 0 {
                let button = chip(format!("Eksikler  ·  {missing_count}"), *missing_only);
                if ui
                    .add(button)
                    .on_hover_text("Sistemde kurulu görünmeyen önerilen paketler.")
                    .clicked()
                {
                    *selected = None;
                    *use_recommended = false;
                    *missing_only = true;
                    *scope_changed = true;
                }
            }

            for category in categories {
                let is_selected = !*use_recommended
                    && !*missing_only
                    && selected.as_deref() == Some(category.id.as_str());
                let button = chip(
                    format!("{}  ·  {}", category.name, category.package_count),
                    is_selected,
                );

                if ui
                    .add(button)
                    .on_hover_text(&category.description)
                    .clicked()
                {
                    *use_recommended = false;
                    *missing_only = false;
                    *selected = if is_selected {
                        None
                    } else {
                        Some(category.id.clone())
                    };
                    *scope_changed = true;
                }
            }
        });

        // Friendly explanation of the active scope, below the chip row.
        let hint = if *missing_only {
            Some("Yalnızca sistemde kurulu görünmeyen önerilen paketler gösteriliyor.".to_string())
        } else if *use_recommended {
            Some(format!(
                "{} profil(ler)inize uygun kategoriler birleştirildi.",
                if recommended_ids.is_empty() {
                    "Donanım".to_string()
                } else {
                    "Eşleşen".to_string()
                }
            ))
        } else if let Some(id) = selected.as_deref() {
            categories
                .iter()
                .find(|category| category.id == id)
                .map(|category| category.description.clone())
        } else {
            None
        };
        if let Some(hint) = hint {
            ui.add_space(8.0);
            ui.label(RichText::new(hint).size(11.5).color(MUTED));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn packages_section(
    ui: &mut egui::Ui,
    packages: &[PackageView],
    scope_label: &str,
    active: Option<&[String]>,
    missing_only: bool,
    checked: &mut HashSet<String>,
    search: &mut String,
    sort: &mut PackageSort,
    low_risk_only: &mut bool,
) {
    card(ui, |ui| {
        section_title(
            ui,
            "Paketler",
            &format!("Görünüm: {scope_label} · kurmak istediklerine tik at."),
        );
        ui.add_space(10.0);

        if !packages.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Ara").size(12.0).color(MUTED));
                ui.add(
                    egui::TextEdit::singleline(search)
                        .hint_text("catalog_id, paket veya açıklama")
                        .desired_width(220.0),
                );
                if !search.is_empty() && ui.small_button("Temizle").clicked() {
                    search.clear();
                }
                ui.add_space(10.0);
                ui.label(RichText::new("Sırala").size(12.0).color(MUTED));
                egui::ComboBox::from_id_salt("package_sort")
                    .selected_text(sort.label())
                    .show_ui(ui, |ui| {
                        for option in [
                            PackageSort::Catalog,
                            PackageSort::Name,
                            PackageSort::Risk,
                            PackageSort::Install,
                        ] {
                            ui.selectable_value(sort, option, option.label());
                        }
                    });
                ui.add_space(10.0);
                ui.checkbox(
                    low_risk_only,
                    RichText::new("Yalnızca düşük risk").size(12.0),
                );
            });
            ui.add_space(6.0);
        }

        let needle = search.trim().to_lowercase();
        let mut visible: Vec<&PackageView> = packages
            .iter()
            .filter(|package| {
                let in_category =
                    active.is_none_or(|ids| ids.iter().any(|id| id == &package.category_id));
                let in_missing = !missing_only || package.install == InstallStatus::Missing;
                let matches = needle.is_empty()
                    || package.catalog_id.to_lowercase().contains(&needle)
                    || package.package.to_lowercase().contains(&needle)
                    || package.reason.to_lowercase().contains(&needle);
                in_category && in_missing && matches
            })
            .filter(|package| !*low_risk_only || package.risk == RiskLevel::Low)
            .collect();

        match sort {
            PackageSort::Catalog => {}
            PackageSort::Name => visible.sort_by(|a, b| {
                a.package
                    .to_lowercase()
                    .cmp(&b.package.to_lowercase())
                    .then_with(|| a.catalog_id.cmp(&b.catalog_id))
            }),
            PackageSort::Risk => visible.sort_by_key(|package| risk_rank(package.risk)),
            PackageSort::Install => visible.sort_by_key(|package| install_rank(package.install)),
        }

        if visible.is_empty() {
            if packages.is_empty() {
                empty_state(ui, "Bu görünümde doğrulanmış paket yok.");
                ui.add_space(8.0);
                guidance(ui);
            } else {
                empty_state(ui, "Bu filtreyle eşleşen paket yok.");
            }
            return;
        }

        // Bulk actions for the currently visible rows: verified entries only,
        // because unverified rows can never reach the plan.
        let selectable: Vec<&str> = visible
            .iter()
            .filter(|package| package.status == VerificationStatus::Verified)
            .map(|package| package.catalog_id.as_str())
            .collect();
        let all_selected =
            !selectable.is_empty() && selectable.iter().all(|id| checked.contains(*id));
        ui.horizontal(|ui| {
            let label = if all_selected {
                "Görünümü temizle"
            } else {
                "Görünümü seç"
            };
            if ui.small_button(label).clicked() {
                if all_selected {
                    for id in &selectable {
                        checked.remove(*id);
                    }
                } else {
                    for id in &selectable {
                        checked.insert((*id).to_string());
                    }
                }
            }
            ui.label(
                RichText::new(format!("{} paket görünüyor", visible.len()))
                    .size(11.0)
                    .color(MUTED),
            );
        });
        ui.add_space(8.0);

        for package in &visible {
            let (risk_text, risk_fg, risk_bg) = risk_badge(package.risk);
            let (status_text, status_fg, status_bg) = status_badge(package.status);
            let installable = package.status == VerificationStatus::Verified;
            let mut is_checked = installable && checked.contains(&package.catalog_id);

            Frame::NONE
                .fill(SURFACE_ALT)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if installable {
                            if ui.checkbox(&mut is_checked, "").changed() {
                                if is_checked {
                                    checked.insert(package.catalog_id.clone());
                                } else {
                                    checked.remove(&package.catalog_id);
                                }
                            }
                        } else {
                            // Unverified rows stay visible but cannot be ticked;
                            // the planner would skip them anyway.
                            ui.add_enabled(false, egui::Checkbox::without_text(&mut false));
                        }
                        ui.label(
                            RichText::new(&package.catalog_id)
                                .size(13.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.add_space(6.0);
                        badge(ui, risk_text, risk_fg, risk_bg);
                        badge(ui, status_text, status_fg, status_bg);
                        if package.install != InstallStatus::Unknown {
                            let (text, fg, bg) = install_badge(package.install);
                            badge(ui, text, fg, bg);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(&package.package)
                                    .monospace()
                                    .size(12.5)
                                    .color(TEXT),
                            );
                        });
                    });

                    if !package.reason.is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new(&package.reason).size(11.5).color(MUTED));
                    }
                    if !package.source.is_empty() {
                        if package.source.starts_with("http") {
                            ui.hyperlink_to(
                                RichText::new(format!("Kaynak: {}", package.source)).size(11.0),
                                &package.source,
                            );
                        } else {
                            ui.label(
                                RichText::new(format!("Kaynak: {}", package.source))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        }
                    }
                });

            ui.add_space(8.0);
        }
    });
}

/// Onboarding guide: what SetupMate does and where the user is in the flow.
fn guide_section(
    ui: &mut egui::Ui,
    system: &SystemInfo,
    scope_chosen: bool,
    selected_count: usize,
    install_finished: bool,
) {
    card(ui, |ui| {
        section_title(
            ui,
            "Rehber",
            "SetupMate bir rehberdir; tek tuşla her şeyi kuran bir araç değildir.",
        );
        ui.add_space(12.0);

        let steps: [(&str, String); 4] = [
            (
                "Sistemi tanı",
                format!(
                    "{} · {} · {} salt-okunur olarak tespit edildi.",
                    distro_label(system.distro_family),
                    gpu_labels(&system.gpu_vendors),
                    session_label(system.session_type),
                ),
            ),
            (
                "Amacını seç",
                "Kategori, Önerilen veya Eksikler ile görünümü daralt; paketler otomatik işaretlenir."
                    .to_string(),
            ),
            (
                "Paketleri işaretle",
                if selected_count > 0 {
                    format!("{selected_count} paket işaretli. Kurmak istemediklerinin tikini kaldır.")
                } else {
                    "Aşağıdaki listeden kurmak istediğin paketlere tik at.".to_string()
                },
            ),
            (
                "Onayla ve kur",
                "Komutlar önce onay penceresinde gösterilir; onaylamadan hiçbiri çalışmaz."
                    .to_string(),
            ),
        ];
        let done = [true, scope_chosen, selected_count > 0, install_finished];
        let current = done.iter().position(|step| !step);

        for (index, (title, detail)) in steps.iter().enumerate() {
            let is_done = done.get(index).copied().unwrap_or(false);
            let is_current = current == Some(index);
            let (marker_bg, marker_fg, title_color) = if is_done {
                (OK_BG, OK, TEXT)
            } else if is_current {
                (ACCENT, ON_ACCENT, TEXT)
            } else {
                (SURFACE_ALT, MUTED, MUTED)
            };

            ui.horizontal(|ui| {
                Frame::NONE
                    .fill(marker_bg)
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        let marker = if is_done {
                            "✓".to_string()
                        } else {
                            format!("{}", index + 1)
                        };
                        ui.label(RichText::new(marker).size(12.5).strong().color(marker_fg));
                    });
                ui.vertical(|ui| {
                    ui.label(RichText::new(*title).size(12.5).strong().color(title_color));
                    ui.label(RichText::new(detail.as_str()).size(11.5).color(MUTED));
                });
            });
            if index + 1 < steps.len() {
                ui.add_space(8.0);
            }
        }
    });
}

/// Recommended packages that are not currently installed, grouped by category.
fn missing_section(
    ui: &mut egui::Ui,
    packages: &[PackageView],
    categories: &[CategoryView],
    install_known: bool,
) {
    card(ui, |ui| {
        section_title(
            ui,
            "Eksik önerilenler",
            "Bu dağıtım için önerilen ama sistemde kurulu görünmeyen paketler.",
        );
        ui.add_space(10.0);

        if !install_known {
            empty_state(
                ui,
                "Bu dağıtım ailesinin paket veritabanı bu sürümde okunamıyor (rpm). \
                 Kurulu/kurulu değil bilgisi gösterilmiyor; hiçbir sonuç uydurulmaz.",
            );
            return;
        }

        let is_missing = |package: &PackageView| {
            package.status == VerificationStatus::Verified
                && package.install == InstallStatus::Missing
        };
        let total = packages
            .iter()
            .filter(|package| is_missing(package))
            .count();

        if total == 0 {
            empty_state(
                ui,
                "Önerilen paketlerin tümü kurulu görünüyor; eklenecek eksik yok.",
            );
            return;
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("{total} paket kurulu değil"))
                    .size(12.5)
                    .strong()
                    .color(TEXT),
            );
            ui.label(
                RichText::new("· eklemek için paketlere tik at; plan otomatik oluşur")
                    .size(11.0)
                    .color(MUTED),
            );
        });
        ui.add_space(10.0);

        for category in categories {
            let group: Vec<&PackageView> = packages
                .iter()
                .filter(|package| package.category_id == category.id && is_missing(package))
                .collect();
            if group.is_empty() {
                continue;
            }
            ui.label(
                RichText::new(format!("{}  ·  {}", category.name, group.len()))
                    .size(12.0)
                    .strong()
                    .color(ACCENT),
            );
            ui.add_space(6.0);
            for package in group {
                missing_row(ui, package);
            }
            ui.add_space(8.0);
        }
    });
}

fn missing_row(ui: &mut egui::Ui, package: &PackageView) {
    let (risk_text, risk_fg, risk_bg) = risk_badge(package.risk);
    Frame::NONE
        .fill(SURFACE_ALT)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&package.package)
                        .monospace()
                        .size(12.5)
                        .strong()
                        .color(TEXT),
                );
                badge(ui, risk_text, risk_fg, risk_bg);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(&package.catalog_id).size(11.0).color(MUTED));
                });
            });
            if !package.reason.is_empty() {
                ui.add_space(3.0);
                ui.label(RichText::new(&package.reason).size(11.0).color(MUTED));
            }
        });
    ui.add_space(6.0);
}

fn plan_section(
    ui: &mut egui::Ui,
    plan: &planner::InstallPlan,
    system: &SystemInfo,
    install: Option<&InstallRun>,
    confirm_requested: &mut bool,
    export_note: &mut Option<String>,
) {
    card(ui, |ui| {
        section_title(
            ui,
            "Kurulum planı",
            "Önce önizleme; komutlar yalnızca onay penceresinden sonra çalışır.",
        );
        ui.add_space(10.0);

        if plan.actions.is_empty() && plan.skipped.is_empty() {
            empty_state(
                ui,
                "Öneri yok. Katalog doğrulanmış paket içerdiğinde plan burada görünecek.",
            );
            return;
        }

        if !plan.actions.is_empty() {
            ui.label(
                RichText::new(format!("Kurulum adımları ({})", plan.actions.len()))
                    .size(12.5)
                    .strong()
                    .color(TEXT),
            );
            ui.add_space(6.0);

            for action in &plan.actions {
                let (risk_text, risk_fg, risk_bg) = risk_badge(action.risk);
                Frame::NONE
                    .fill(SURFACE_ALT)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(12, 9))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&action.catalog_id)
                                    .size(12.5)
                                    .strong()
                                    .color(TEXT),
                            );
                            badge(ui, risk_text, risk_fg, risk_bg);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(action.command.join(" "))
                                        .monospace()
                                        .size(11.5)
                                        .color(MUTED),
                                );
                            });
                        });
                    });
                ui.add_space(6.0);
            }

            ui.add_space(6.0);
            let running = install.is_some_and(InstallRun::running);
            if running {
                let (done, total) = install
                    .map(|run| (run.done, run.total))
                    .unwrap_or((0, plan.actions.len()));
                let fraction = if total == 0 {
                    0.0
                } else {
                    (done as f32 / total as f32).clamp(0.0, 1.0)
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(format!("Kurulum sürüyor: {done}/{total}"))
                        .desired_width(ui.available_width().min(520.0)),
                );
                ui.label(
                    RichText::new("Komutlar yönetici yetkisiyle çalışıyor; pencereyi kapatma.")
                        .size(11.0)
                        .color(MUTED),
                );
            } else if primary_button(ui, "Kurulumu başlat").clicked() {
                *confirm_requested = true;
            }
            if !running {
                ui.label(
                    RichText::new(
                        "Düğme yalnızca onay penceresini açar; orada komutları görüp onaylaman gerekir.",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
            }

            if let Some(run) = install {
                install_result(ui, run);
            }

            // Risk dağılımı: onaydan sonra planın ne getirdiğini sayısal olarak özetler.
            let low = plan
                .actions
                .iter()
                .filter(|action| action.risk == RiskLevel::Low)
                .count();
            let medium = plan
                .actions
                .iter()
                .filter(|action| action.risk == RiskLevel::Medium)
                .count();
            let high = plan
                .actions
                .iter()
                .filter(|action| action.risk == RiskLevel::High)
                .count();
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Risk dağılımı:").size(11.5).color(MUTED));
                badge(ui, &format!("düşük {low}"), OK, OK_BG);
                badge(ui, &format!("orta {medium}"), WARN, WARN_BG);
                badge(ui, &format!("yüksek {high}"), DANGER, DANGER_BG);
            });

            // Export the plan as plain text the user can copy or save. This is a
            // read-only convenience: nothing is executed and no config is touched.
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Planı kopyala")
                    .on_hover_text("Komutları panoya kopyalar; terminale kendin yapıştırırsın.")
                    .clicked()
                {
                    ui.ctx().copy_text(plan_export_text(plan, system));
                    *export_note = Some("Plan panoya kopyalandı.".to_string());
                }
                if ui
                    .button("Planı kaydet")
                    .on_hover_text("Planı bir metin dosyasına yazar; hiçbir komut çalıştırılmaz.")
                    .clicked()
                {
                    match save_plan_file(&plan_export_text(plan, system)) {
                        Ok(path) => {
                            *export_note = Some(format!("Kaydedildi: {}", path.display()));
                        }
                        Err(error) => {
                            *export_note = Some(format!("Kaydedilemedi: {error}"));
                        }
                    }
                }
            });
            if let Some(note) = export_note.as_deref() {
                ui.label(RichText::new(note).size(11.0).color(MUTED));
            }
        }

        if !plan.skipped.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("Plandan çıkarılanlar ({})", plan.skipped.len()))
                    .size(12.5)
                    .strong()
                    .color(TEXT),
            );
            ui.add_space(6.0);

            for item in &plan.skipped {
                Frame::NONE
                    .fill(SURFACE_ALT)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&item.catalog_id)
                                    .size(12.0)
                                    .strong()
                                    .color(TEXT),
                            );
                            ui.label(RichText::new(item.reason.message()).size(11.0).color(WARN));
                        });
                    });
                ui.add_space(6.0);
            }
        }
    });
}

/// Shows the outcome and recent log lines of a live install.
fn install_result(ui: &mut egui::Ui, run: &InstallRun) {
    ui.add_space(8.0);

    if let Some(error) = &run.error {
        badge(ui, "KURULUM BAŞARISIZ", DANGER, DANGER_BG);
        ui.add_space(4.0);
        ui.label(RichText::new(error).size(11.5).color(DANGER));
    } else if run.finished {
        let ok = run.report.as_ref().is_some_and(ExecutionReport::succeeded);
        if ok {
            badge(ui, "KURULUM TAMAMLANDI", OK, OK_BG);
        } else {
            badge(ui, "KURULUM DURDU", WARN, WARN_BG);
        }
        ui.label(
            RichText::new("Kurulu durum yenilendi; paket rozetleri güncellendi.")
                .size(11.0)
                .color(MUTED),
        );
    }

    if run.log.is_empty() {
        return;
    }

    ui.add_space(6.0);
    ui.label(
        RichText::new(format!("İşlem günlüğü ({}/{})", run.done, run.total))
            .size(12.0)
            .strong()
            .color(TEXT),
    );
    ui.add_space(4.0);

    ScrollArea::vertical()
        .id_salt("install_log")
        .max_height(220.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in run.log.iter().rev().take(40) {
                let (marker, color) = if line.ok {
                    ("✓", OK)
                } else {
                    ("✗", DANGER)
                };
                Frame::NONE
                    .fill(SURFACE_ALT)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(marker).size(12.0).strong().color(color));
                            ui.label(
                                RichText::new(&line.catalog_id)
                                    .size(11.5)
                                    .strong()
                                    .color(TEXT),
                            );
                            ui.label(
                                RichText::new(&line.command)
                                    .monospace()
                                    .size(10.5)
                                    .color(MUTED),
                            );
                        });
                        if !line.ok {
                            ui.label(RichText::new(&line.message).size(10.5).color(DANGER));
                        }
                    });
                ui.add_space(4.0);
            }
        });
}

/// Explicit approval dialog. Returns the user's decision for this frame.
fn confirm_modal(ctx: &egui::Context, live: &planner::InstallPlan) -> ConfirmAction {
    let mut action = ConfirmAction::None;

    egui::Window::new("Kurulumu onayla")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(620.0);
            ui.label(
                RichText::new(
                    "Aşağıdaki komutlar yönetici yetkisiyle çalıştırılacak. \
                     Devam etmeden önce listeyi kontrol et.",
                )
                .size(12.5)
                .color(TEXT),
            );
            ui.add_space(8.0);

            ScrollArea::vertical()
                .id_salt("confirm_commands")
                .max_height(260.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (index, step) in live.actions.iter().enumerate() {
                        Frame::NONE
                            .fill(SURFACE_ALT)
                            .stroke(Stroke::new(1.0, BORDER))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(10, 7))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(format!("{}.", index + 1))
                                            .size(11.5)
                                            .color(MUTED),
                                    );
                                    ui.label(
                                        RichText::new(&step.catalog_id)
                                            .size(11.5)
                                            .strong()
                                            .color(TEXT),
                                    );
                                    ui.label(
                                        RichText::new(step.command.join(" "))
                                            .monospace()
                                            .size(10.5)
                                            .color(MUTED),
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }
                });

            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "SetupMate yalnızca bu komutları çalıştırır; paket kaldırmaz, \
                     repo/anahtar eklemez, yapılandırma dosyalarını değiştirmez.",
                )
                .size(11.0)
                .color(MUTED),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Vazgeç").clicked() {
                    action = ConfirmAction::Cancel;
                }
                if primary_button(ui, "Onayla ve kur").clicked() {
                    action = ConfirmAction::Install;
                }
            });
        });

    action
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

fn distro_label(family: DistroFamily) -> &'static str {
    match family {
        DistroFamily::Debian => "Debian / Ubuntu",
        DistroFamily::Fedora => "Fedora / RHEL",
        DistroFamily::Arch => "Arch",
        DistroFamily::OpenSuse => "openSUSE",
        DistroFamily::Alpine => "Alpine",
        DistroFamily::Void => "Void",
        DistroFamily::Unknown => "Bilinmiyor",
    }
}

fn gpu_label(vendor: GpuVendor) -> &'static str {
    match vendor {
        GpuVendor::Nvidia => "NVIDIA",
        GpuVendor::Amd => "AMD",
        GpuVendor::Intel => "Intel",
        GpuVendor::Unknown => "Bilinmiyor",
    }
}

/// Renders every detected GPU vendor (hybrid machines list both), or
/// "Bilinmiyor" when none was found.
fn gpu_labels(vendors: &[GpuVendor]) -> String {
    let known: Vec<&str> = vendors
        .iter()
        .filter(|vendor| **vendor != GpuVendor::Unknown)
        .map(|vendor| gpu_label(*vendor))
        .collect();
    if known.is_empty() {
        "Bilinmiyor".to_string()
    } else {
        known.join(" + ")
    }
}

fn form_factor_label(form: FormFactor) -> &'static str {
    match form {
        FormFactor::Laptop => "Dizüstü",
        FormFactor::Desktop => "Masaüstü",
        FormFactor::Unknown => "Bilinmiyor",
    }
}

fn session_label(session: SessionType) -> &'static str {
    match session {
        SessionType::Wayland => "Wayland",
        SessionType::X11 => "X11",
        SessionType::Unknown => "Bilinmiyor",
    }
}

fn risk_badge(risk: RiskLevel) -> (&'static str, Color32, Color32) {
    match risk {
        RiskLevel::Low => ("düşük risk", OK, OK_BG),
        RiskLevel::Medium => ("orta risk", WARN, WARN_BG),
        RiskLevel::High => ("yüksek risk", DANGER, DANGER_BG),
    }
}

fn status_badge(status: VerificationStatus) -> (&'static str, Color32, Color32) {
    match status {
        VerificationStatus::Verified => ("doğrulandı", OK, OK_BG),
        VerificationStatus::TodoVerify => ("TODO_VERIFY", WARN, WARN_BG),
    }
}

fn install_badge(status: InstallStatus) -> (&'static str, Color32, Color32) {
    match status {
        InstallStatus::Installed => ("kurulu", OK, OK_BG),
        InstallStatus::Missing => ("eksik", WARN, WARN_BG),
        InstallStatus::Unknown => ("bilinmiyor", MUTED, SURFACE_ALT),
    }
}

/// Sort key: low risk first.
fn risk_rank(risk: RiskLevel) -> u8 {
    match risk {
        RiskLevel::Low => 0,
        RiskLevel::Medium => 1,
        RiskLevel::High => 2,
    }
}

/// Sort key: not-installed entries first (these are the ones worth adding).
fn install_rank(status: InstallStatus) -> u8 {
    match status {
        InstallStatus::Missing => 0,
        InstallStatus::Installed => 1,
        InstallStatus::Unknown => 2,
    }
}

/// Plain-text rendering of a plan for copy/save. Never contains anything that
/// would be executed by SetupMate itself.
fn plan_export_text(plan: &planner::InstallPlan, system: &SystemInfo) -> String {
    let mut out = String::new();
    out.push_str(&format!("SetupMate {VERSION} — kurulum planı (önizleme)\n"));
    out.push_str(&format!(
        "Dağıtım: {} · {} · {}\n",
        distro_label(system.distro_family),
        gpu_labels(&system.gpu_vendors),
        session_label(system.session_type),
    ));
    out.push_str(
        "Not: Bu dosya yalnızca önizlemedir. SetupMate komutları yalnızca uygulamada \
         onay penceresini kabul ettiğinde çalıştırır.\n\n",
    );

    if plan.actions.is_empty() {
        out.push_str("Kurulum adımı yok.\n");
    } else {
        out.push_str(&format!("Adımlar ({}):\n", plan.actions.len()));
        for (index, action) in plan.actions.iter().enumerate() {
            out.push_str(&format!(
                "{:>2}. {}  [{}]\n      {}\n",
                index + 1,
                action.catalog_id,
                risk_badge(action.risk).0,
                action.command.join(" ")
            ));
        }
    }

    if !plan.skipped.is_empty() {
        out.push_str(&format!(
            "\nPlandan çıkarılanlar ({}):\n",
            plan.skipped.len()
        ));
        for item in &plan.skipped {
            out.push_str(&format!(
                "- {} : {}\n",
                item.catalog_id,
                item.reason.message()
            ));
        }
    }
    out
}

/// Writes the plan text to the app's data directory (XDG). Returns the path.
/// This is a user-initiated export artifact; no system config is modified.
fn save_plan_file(text: &str) -> Result<PathBuf, std::io::Error> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            home.join(".local/share")
        });
    let dir = base.join("setupmate");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("setupmate-plan.txt");
    std::fs::write(&path, text)?;
    Ok(path)
}

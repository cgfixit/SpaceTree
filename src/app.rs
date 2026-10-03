//! Native window: path, background scan, size columns, and treemap.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use eframe::egui::{
    self, Align2, Color32, FontId, Key, Label, Mesh, Pos2, ScrollArea, Sense, Stroke, TextEdit,
};
use egui_extras::{Column, TableBuilder};

use crate::finder::reveal_in_finder;
use crate::{
    ext_color, ext_description, ext_label, format_bytes, format_scan_share, layout_node, legend_of,
    share_px, sort_tree, LegendRow, Node, PxRect, ScanResult, SortColumn, Tiling,
};

pub const APP_TITLE: &str = "SpaceTree";

enum Phase {
    Idle,
    Scanning(PathBuf),
    Ready,
    Failed(String),
}

struct MapCache {
    generation: u64,
    focus: PathBuf,
    bounds: PxRect,
    tiling: Tiling,
}

struct FinishedScan {
    result: ScanResult,
    legend: Vec<LegendRow>,
    sort: SortColumn,
    descending: bool,
}

struct SpaceTreeApp {
    path_text: String,
    phase: Phase,
    result: Option<ScanResult>,
    rx: Option<Receiver<Result<FinishedScan, String>>>,
    expanded: HashSet<PathBuf>,
    sort: SortColumn,
    descending: bool,
    autoscan: bool,
    selected: Option<PathBuf>,
    path_focused: bool,
    last_note: Option<String>,
    split_top: f32,
    legend: Vec<LegendRow>,
    generation: u64,
    map_cache: Option<MapCache>,
    zoom: Option<PathBuf>,
    /// Extension key under the pointer in the legend; the map dims the rest.
    legend_hover: Option<String>,
}

impl SpaceTreeApp {
    fn new() -> Self {
        let autoscan = std::env::var("SPACETREE_AUTOSCAN").ok();
        Self {
            path_text: autoscan.clone().unwrap_or_else(default_path),
            phase: Phase::Idle,
            result: None,
            rx: None,
            expanded: HashSet::new(),
            sort: SortColumn::Size,
            descending: true,
            autoscan: autoscan.is_some(),
            selected: None,
            path_focused: false,
            last_note: None,
            split_top: 0.42,
            legend: Vec::new(),
            generation: 0,
            map_cache: None,
            zoom: None,
            legend_hover: None,
        }
    }

    fn start_scan(&mut self, ctx: &egui::Context) {
        if matches!(self.phase, Phase::Scanning(_)) {
            return;
        }
        let trimmed = self.path_text.trim();
        if trimmed.is_empty() {
            self.phase = Phase::Failed("path is empty".into());
            return;
        }
        let path = PathBuf::from(trimmed);
        self.phase = Phase::Scanning(path.clone());
        self.result = None;
        self.selected = None;
        self.last_note = None;
        self.expanded.clear();
        self.legend.clear();
        self.map_cache = None;
        self.zoom = None;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        let sort = self.sort;
        let descending = self.descending;
        thread::Builder::new()
            .name("spacetree-scan".into())
            .spawn(move || {
                let msg = match crate::scan(&path) {
                    Ok(mut result) => {
                        sort_tree(&mut result.root, sort, descending);
                        let legend = legend_of(&result.root);
                        Ok(FinishedScan {
                            result,
                            legend,
                            sort,
                            descending,
                        })
                    }
                    Err(e) => Err(format!("{}: {e}", path.display())),
                };
                let _ = tx.send(msg);
                ctx.request_repaint();
            })
            .expect("scan thread");
    }

    fn poll_scan(&mut self) {
        let Some(rx) = self.rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(mut finished)) => {
                if finished.sort != self.sort || finished.descending != self.descending {
                    sort_tree(&mut finished.result.root, self.sort, self.descending);
                }
                self.legend = finished.legend;
                self.zoom = None;
                self.generation = self.generation.wrapping_add(1);
                self.map_cache = None;
                self.expanded.clear();
                self.expanded.insert(finished.result.root.path.clone());
                self.result = Some(finished.result);
                self.phase = Phase::Ready;
            }
            Ok(Err(e)) => {
                self.result = None;
                self.phase = Phase::Failed(e);
            }
            Err(TryRecvError::Empty) => {
                self.rx = Some(rx);
            }
            Err(TryRecvError::Disconnected) => {
                self.phase = Phase::Failed("scan thread ended unexpectedly".into());
            }
        }
    }
}

pub fn run() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title(APP_TITLE),
        ..Default::default()
    };
    eframe::run_native(
        APP_TITLE,
        opts,
        Box::new(|_cc| Ok(Box::new(SpaceTreeApp::new()))),
    )
}

impl eframe::App for SpaceTreeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.autoscan {
            self.autoscan = false;
            self.start_scan(ctx);
        }
        self.poll_scan();

        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if !self.path_focused {
            if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::C)) {
                self.copy_selected(ctx);
            }
            if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::O)) {
                self.reveal_selected();
            }
            if self.zoom.is_some() && ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.zoom_out();
            }
        }
        let enter = ctx.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.command);
        let scanning = matches!(self.phase, Phase::Scanning(_));

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui
                        .add_enabled(
                            self.selected.is_some(),
                            egui::Button::new("Reveal in Finder").shortcut_text("⌘O"),
                        )
                        .clicked()
                    {
                        self.reveal_selected();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            self.selected.is_some(),
                            egui::Button::new("Copy Path").shortcut_text("⌘C"),
                        )
                        .clicked()
                    {
                        self.copy_selected(ui.ctx());
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new("Quit").shortcut_text("⌘Q"))
                        .clicked()
                    {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close_menu();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!scanning, egui::Button::new("Scan"))
                        .clicked()
                        || enter
                    {
                        self.start_scan(ctx);
                    }
                    if ui.button("Browse…").clicked() && !scanning {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.path_text = p.to_string_lossy().into_owned();
                        }
                    }
                    let edit = ui.add(
                        TextEdit::singleline(&mut self.path_text)
                            .hint_text("Folder to scan")
                            .desired_width(ui.available_width()),
                    );
                    self.path_focused = edit.has_focus();
                });
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| match &self.phase {
                Phase::Scanning(path) => {
                    ui.label(format!("Scanning… {}", path.display()));
                }
                Phase::Failed(e) => {
                    ui.colored_label(Color32::RED, e);
                }
                Phase::Ready => {
                    if let Some(result) = &self.result {
                        ui.label(format!("walkable {}", format_bytes(result.root.size)))
                            .on_hover_text(format!("{} bytes", result.root.size));
                        ui.separator();
                        ui.label(format!("volume {}", format_bytes(result.volume_total)))
                            .on_hover_text(format!("{} bytes", result.volume_total));
                        ui.separator();
                        ui.label(format_percent(result.root.percent_of_disk));
                        if let Some(sel) = &self.selected {
                            ui.separator();
                            if let Some(node) = find_node(&result.root, sel) {
                                ui.label(format!("selected {}", format_bytes(node.size)))
                                    .on_hover_text(format!("{} bytes", node.size));
                            }
                            ui.label(sel.display().to_string());
                        }
                    }
                }
                Phase::Idle => {
                    ui.label("Ready");
                }
            });
            if let Some(result) = &self.result {
                if result.error_count > 0 {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "Scan incomplete: {} read {}. Sizes are partial.",
                            result.error_count,
                            if result.error_count == 1 {
                                "error"
                            } else {
                                "errors"
                            }
                        ),
                    );
                }
            }
            if let Some(note) = &self.last_note {
                ui.separator();
                ui.label(note);
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if !matches!(self.phase, Phase::Ready) {
                match &self.phase {
                    Phase::Scanning(path) => {
                        ui.label(format!("Scanning… {}", path.display()));
                    }
                    Phase::Failed(e) => {
                        ui.colored_label(Color32::RED, e);
                    }
                    Phase::Idle | Phase::Ready => {}
                }
                return;
            }
            let full_h = ui.available_height().max(1.0);
            let top_h = (full_h * self.split_top).clamp(120.0, (full_h - 90.0).max(120.0));
            let full_w = ui.available_width();
            let legend_w = 180.0_f32.min(full_w * 0.28).max(140.0);
            let gap = 8.0;
            let table_w = (full_w - legend_w - gap).max(200.0);
            let origin = ui.cursor().min;
            let table_rect = egui::Rect::from_min_size(origin, egui::vec2(table_w, top_h));
            let legend_rect = egui::Rect::from_min_size(
                egui::pos2(origin.x + table_w + gap, origin.y),
                egui::vec2(legend_w, top_h),
            );
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(table_rect), |ui| {
                ui.set_clip_rect(table_rect);
                self.table(ui);
            });
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(legend_rect), |ui| {
                ui.set_clip_rect(legend_rect);
                self.legend_panel(ui);
            });
            let handle = ui.allocate_response(egui::vec2(ui.available_width(), 6.0), Sense::drag());
            if handle.dragged() {
                self.split_top =
                    (self.split_top + handle.drag_delta().y / full_h).clamp(0.20, 0.80);
            }
            let bar = if handle.hovered() || handle.dragged() {
                ui.visuals().widgets.hovered.bg_fill
            } else {
                ui.visuals().widgets.noninteractive.bg_stroke.color
            };
            ui.painter().rect_filled(handle.rect, 0.0, bar);
            self.treemap(ui);
        });
    }
}

impl SpaceTreeApp {
    fn copy_selected(&mut self, ctx: &egui::Context) {
        let Some(path) = &self.selected else {
            return;
        };
        let s = path.display().to_string();
        ctx.copy_text(s.clone());
        self.last_note = Some(format!("copied {s}"));
    }

    fn reveal_selected(&mut self) {
        let Some(path) = &self.selected else {
            return;
        };
        match reveal_in_finder(path) {
            Ok(()) => self.last_note = Some(format!("revealed {}", path.display())),
            Err(e) => self.last_note = Some(e),
        }
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        let mut toggles = Vec::new();
        let mut new_sort = None;
        let mut clicked: Option<PathBuf> = None;
        let mut dbl: Option<PathBuf> = None;
        let mut ctx_copy: Option<PathBuf> = None;
        let mut ctx_reveal: Option<PathBuf> = None;
        {
            let Some(result) = &self.result else {
                return;
            };
            let mut visible = Vec::new();
            flatten(&result.root, 0, &self.expanded, &mut visible);
            let selected = self.selected.clone();
            let weak = ui.visuals().weak_text_color();
            let root_size = result.root.size;

            let mut header = |ui: &mut egui::Ui, col: SortColumn, label: &str, show_mark: bool| {
                let mark = if show_mark && self.sort == col {
                    if self.descending {
                        " ⏷"
                    } else {
                        " ⏶"
                    }
                } else {
                    ""
                };
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(format!("{label}{mark}")).strong(),
                    ))
                    .clicked()
                {
                    new_sort = Some(col);
                }
            };

            let pane_h = ui.available_height().max(80.0);
            ScrollArea::horizontal()
                .id_salt("tree-hscroll")
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    ui.set_min_width(790.0);
                    ui.set_height(pane_h);
                    TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .sense(Sense::click())
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .min_scrolled_height((pane_h - 28.0).max(80.0))
                        .max_scroll_height(pane_h)
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .column(Column::initial(280.0).at_least(140.0).clip(true))
                        .column(Column::initial(120.0).at_least(72.0).clip(true))
                        .column(Column::initial(84.0).at_least(64.0).clip(true))
                        .column(Column::initial(110.0).at_least(80.0).clip(true))
                        .column(Column::initial(110.0).at_least(80.0).clip(true))
                        .column(Column::initial(72.0).at_least(52.0).clip(true))
                        .header(22.0, |mut row| {
                            row.col(|ui| header(ui, SortColumn::Name, "Name", true));
                            row.col(|ui| header(ui, SortColumn::Size, "Size Proportion", false));
                            row.col(|ui| header(ui, SortColumn::Size, "Percentage", false));
                            row.col(|ui| header(ui, SortColumn::Size, "Physical Size", true));
                            row.col(|ui| header(ui, SortColumn::Logical, "Logical Size", true));
                            row.col(|ui| header(ui, SortColumn::Files, "Files", true));
                        })
                        .body(|body| {
                            body.rows(20.0, visible.len(), |mut row| {
                                let (node, depth) = visible[row.index()];
                                let is_sel = selected.as_ref() == Some(&node.path);
                                row.set_selected(is_sel);
                                row.col(|ui| {
                                    ui.add_space(depth as f32 * 14.0);
                                    if node.is_dir {
                                        let open = self.expanded.contains(&node.path);
                                        let tri = if open { "⏷" } else { "⏵" };
                                        let r = ui.add(
                                            Label::new(format!("{tri}  {}", node.name))
                                                .sense(Sense::click())
                                                .truncate(),
                                        );
                                        if r.clicked() {
                                            toggles.push(node.path.clone());
                                        }
                                    } else {
                                        ui.add_space(16.0);
                                        match fs::read_link(&node.path) {
                                            Ok(target) => {
                                                ui.colored_label(
                                                    weak,
                                                    format!("{} → {}", node.name, target.display()),
                                                );
                                            }
                                            Err(_) => {
                                                ui.add(Label::new(&node.name).truncate());
                                            }
                                        }
                                    }
                                });
                                row.col(|ui| {
                                    proportion_bar(ui, node.size, root_size, &node.color_ext);
                                });
                                row.col(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.monospace(format_scan_share(node.size, root_size));
                                        },
                                    );
                                });
                                row.col(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.monospace(format_bytes(node.size))
                                                .on_hover_text(format!("{} bytes", node.size));
                                        },
                                    );
                                });
                                row.col(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.monospace(format_bytes(node.logical))
                                                .on_hover_text(format!("{} bytes", node.logical));
                                        },
                                    );
                                });
                                row.col(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.monospace(node.files.to_string());
                                        },
                                    );
                                });
                                let r = row.response();
                                if r.clicked() {
                                    clicked = Some(node.path.clone());
                                }
                                if r.double_clicked() {
                                    dbl = Some(node.path.clone());
                                }
                                r.context_menu(|ui| {
                                    if ui.button("Reveal in Finder").clicked() {
                                        ctx_reveal = Some(node.path.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("Copy Path").clicked() {
                                        ctx_copy = Some(node.path.clone());
                                        ui.close_menu();
                                    }
                                });
                            });
                        });
                });
        }

        if let Some(p) = clicked {
            self.selected = Some(p);
        }
        if let Some(p) = dbl {
            let is_dir = self
                .result
                .as_ref()
                .and_then(|result| find_node(&result.root, &p))
                .is_some_and(|node| node.is_dir);
            self.selected = Some(p.clone());
            if is_dir {
                self.zoom = Some(p);
                self.map_cache = None;
            } else {
                self.reveal_selected();
            }
        }
        if let Some(p) = ctx_reveal {
            self.selected = Some(p);
            self.reveal_selected();
        }
        if let Some(p) = ctx_copy {
            self.selected = Some(p);
            self.copy_selected(ui.ctx());
        }
        for p in toggles {
            if !self.expanded.remove(&p) {
                self.expanded.insert(p);
            }
        }
        if let Some(col) = new_sort {
            if self.sort == col {
                self.descending = !self.descending;
            } else {
                self.sort = col;
                self.descending = col != SortColumn::Name;
            }
            if let Some(result) = &mut self.result {
                sort_tree(&mut result.root, self.sort, self.descending);
            }
        }
    }

    fn legend_panel(&mut self, ui: &mut egui::Ui) {
        let root_size = self.result.as_ref().map_or(0, |r| r.root.size);
        let mut hovered = None;
        ui.label(egui::RichText::new("Extension").strong())
            .on_hover_text("Hover a row to highlight its tiles in the map");
        ScrollArea::vertical()
            .id_salt("ext-legend")
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                for row in &self.legend {
                    let r = ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let (swatch, _) =
                                ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                            let rgb = ext_color(&row.key);
                            ui.painter().rect_filled(
                                swatch,
                                3.0,
                                Color32::from_rgb(rgb.r, rgb.g, rgb.b),
                            );
                            ui.add(Label::new(ext_label(&row.key)).truncate());
                        });
                        ui.add(
                            Label::new(
                                egui::RichText::new(format!(
                                    "{}  ·  {}",
                                    format_bytes(row.physical),
                                    format_scan_share(row.physical, root_size)
                                ))
                                .monospace()
                                .small(),
                            )
                            .truncate(),
                        );
                        ui.add(
                            Label::new(egui::RichText::new(ext_description(&row.key)).weak())
                                .truncate(),
                        );
                    });
                    if ui.rect_contains_pointer(r.response.rect) {
                        hovered = Some(row.key.clone());
                        ui.painter().rect_stroke(
                            r.response.rect.expand(2.0),
                            3.0,
                            Stroke::new(1.0_f32, ui.visuals().widgets.hovered.bg_stroke.color),
                            egui::StrokeKind::Outside,
                        );
                    }
                    ui.add_space(4.0);
                }
            });
        self.legend_hover = hovered;
    }

    fn treemap(&mut self, ui: &mut egui::Ui) {
        let Some(result) = &self.result else {
            return;
        };
        let root_size = result.root.size;
        let focus = self.map_focus(&result.root);
        let focus_path = focus.path.clone();
        let focus_size = focus.size;
        let focus_files = focus.files;
        let crumbs = breadcrumbs(&result.root, &focus_path);

        let mut go_to: Option<Option<PathBuf>> = None;
        let header_h = 26.0;
        let (header, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), header_h), Sense::hover());
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(header.shrink2(egui::vec2(4.0, 0.0))),
            |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let zoomed = self.zoom.is_some();
                    if ui
                        .add_enabled(zoomed, egui::Button::new("⬆ Zoom out"))
                        .on_hover_text("Back to the parent folder (Esc)")
                        .clicked()
                    {
                        go_to = Some(None);
                    }
                    ui.separator();
                    let last = crumbs.len().saturating_sub(1);
                    for (i, (path, name)) in crumbs.iter().enumerate() {
                        if i > 0 {
                            ui.label(egui::RichText::new("›").weak());
                        }
                        let text = egui::RichText::new(name);
                        let text = if i == last { text.strong() } else { text };
                        if ui
                            .add(egui::Button::new(text).frame(false))
                            .on_hover_text(path.display().to_string())
                            .clicked()
                            && i != last
                        {
                            go_to = Some(Some(path.clone()));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}  ·  {} of scan  ·  {} files",
                                format_bytes(focus_size),
                                format_scan_share(focus_size, root_size),
                                focus_files
                            ))
                            .monospace()
                            .weak(),
                        );
                    });
                });
            },
        );
        if let Some(target) = go_to {
            match target {
                None => self.zoom_out(),
                Some(path) => {
                    let is_root = self.result.as_ref().is_some_and(|r| r.root.path == path);
                    self.zoom = if is_root { None } else { Some(path) };
                    self.map_cache = None;
                }
            }
            return;
        }

        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        let bounds = px_from_rect(rect);
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, MAP_BG);
        if focus_size == 0 {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "No allocated bytes to draw here",
                FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
            return;
        }
        let stale = self.map_cache.as_ref().is_none_or(|cache| {
            cache.generation != self.generation
                || cache.focus != focus_path
                || cache.bounds != bounds
        });
        if stale {
            let tiling = self
                .result
                .as_ref()
                .map(|r| layout_node(self.map_focus(&r.root), bounds));
            if let Some(tiling) = tiling {
                self.map_cache = Some(MapCache {
                    generation: self.generation,
                    focus: focus_path.clone(),
                    bounds,
                    tiling,
                });
            }
        }
        let Some(cache) = &self.map_cache else {
            return;
        };
        let tiling = &cache.tiling;
        let pointer = response.hover_pos().filter(|p| rect.contains(*p));
        let (px, py) = pointer
            .map(|p| (p.x.floor() as i32, p.y.floor() as i32))
            .unwrap_or((i32::MIN, i32::MIN));
        let hovered_tile = pointer.and_then(|_| tiling.hit_tile(px, py));
        let hovered_top = pointer.and_then(|_| tiling.frames_at(px, py).find(|f| f.depth == 1));
        let selected = self.selected.as_deref();
        let legend_key = self.legend_hover.as_deref();

        // Tiles: allocated-byte area, extension color, soft top-left light.
        for tile in tiling.tiles() {
            let full = screen_rect(tile.rect);
            let paint = if tile.rect.w > 3 && tile.rect.h > 3 {
                full.shrink(0.5)
            } else {
                full
            };
            if tile.merged {
                paint_other(&painter, paint, tile.weight);
                continue;
            }
            let rgb = ext_color(&tile.color_ext);
            let mut color = Color32::from_rgb(rgb.r, rgb.g, rgb.b);
            if legend_key.is_some_and(|k| k != tile.color_ext) {
                color = mix(color, MAP_BG, 0.72);
            }
            if paint.width() >= 6.0 && paint.height() >= 6.0 {
                paint_cushion(&painter, paint, color);
            } else if paint.width() > 0.0 && paint.height() > 0.0 {
                painter.rect_filled(paint, 0.0, color);
            }
        }

        // Folder outlines: strong for the focus's children, light for grandchildren.
        for frame in tiling.frames() {
            let r = screen_rect(frame.rect);
            match frame.depth {
                1 => {
                    painter.rect_stroke(
                        r,
                        0.0,
                        Stroke::new(2.0_f32, FRAME_LINE),
                        egui::StrokeKind::Inside,
                    );
                }
                2 => {
                    painter.rect_stroke(
                        r,
                        0.0,
                        Stroke::new(1.0_f32, FRAME_LINE.gamma_multiply(0.55)),
                        egui::StrokeKind::Inside,
                    );
                }
                _ => {}
            }
        }

        // Labels: folder name and size in a pill; large files get name and size.
        let mut pills: Vec<(egui::Rect, &Path)> = Vec::new();
        for frame in tiling.frames().iter().filter(|f| f.depth == 1) {
            if frame.rect.w < 72 || frame.rect.h < 30 {
                continue;
            }
            let r = screen_rect(frame.rect);
            let galley = one_line(
                &painter,
                format!("{}  {}", file_name(&frame.path), format_bytes(frame.weight)),
                FontId::proportional(12.0),
                Color32::WHITE,
                r.width() - 14.0,
            );
            let pill = egui::Rect::from_min_size(
                r.left_top() + egui::vec2(3.0, 3.0),
                galley.size() + egui::vec2(10.0, 4.0),
            );
            let hot = hovered_top.is_some_and(|f| f.path == frame.path);
            painter.rect_filled(pill, 4.0, if hot { PILL_HOT } else { PILL_BG });
            painter.galley(pill.min + egui::vec2(5.0, 2.0), galley, Color32::WHITE);
            pills.push((pill, frame.path.as_path()));
        }
        for tile in tiling.tiles() {
            if tile.merged || tile.rect.w < 64 || tile.rect.h < 30 {
                continue;
            }
            let r = screen_rect(tile.rect).shrink(5.0);
            let rgb = ext_color(&tile.color_ext);
            let ink = if luminance(rgb.r, rgb.g, rgb.b) > 150.0 {
                Color32::from_rgb(22, 24, 30)
            } else {
                Color32::WHITE
            };
            let name = one_line(
                &painter,
                file_name(&tile.path),
                FontId::proportional(12.0),
                ink,
                r.width(),
            );
            let size = (tile.rect.h >= 44).then(|| {
                one_line(
                    &painter,
                    format_bytes(tile.weight),
                    FontId::monospace(11.0),
                    ink.gamma_multiply(0.8),
                    r.width(),
                )
            });
            let block_h = name.size().y + size.as_ref().map_or(0.0, |g| g.size().y);
            let top = egui::pos2(r.left(), r.bottom() - block_h);
            let block = egui::Rect::from_min_size(top, egui::vec2(r.width(), block_h));
            if pills.iter().any(|(p, _)| p.intersects(block)) {
                continue;
            }
            let name_h = name.size().y;
            painter.galley(top, name, ink);
            if let Some(size) = size {
                painter.galley(top + egui::vec2(0.0, name_h), size, ink);
            }
        }

        // Selection and hover outlines on top.
        if let Some(sel) = selected {
            for frame in tiling.frames().iter().filter(|f| f.path == sel) {
                painter.rect_stroke(
                    screen_rect(frame.rect),
                    0.0,
                    Stroke::new(2.5_f32, SELECT),
                    egui::StrokeKind::Inside,
                );
            }
            for tile in tiling.tiles().iter().filter(|t| t.path == sel && !t.merged) {
                let r = screen_rect(tile.rect);
                painter.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(3.0_f32, Color32::from_black_alpha(200)),
                    egui::StrokeKind::Inside,
                );
                painter.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(1.5_f32, SELECT),
                    egui::StrokeKind::Inside,
                );
            }
        }
        if let Some(frame) = hovered_top {
            painter.rect_stroke(
                screen_rect(frame.rect),
                0.0,
                Stroke::new(1.0_f32, Color32::from_white_alpha(90)),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(tile) = hovered_tile {
            painter.rect_stroke(
                screen_rect(tile.rect),
                0.0,
                Stroke::new(1.5_f32, Color32::from_white_alpha(220)),
                egui::StrokeKind::Inside,
            );
        }

        let pill_hit = pointer.and_then(|p| {
            pills
                .iter()
                .find(|(r, _)| r.contains(p))
                .map(|(_, path)| path.to_path_buf())
        });
        let tip = hovered_tile.map(|tile| {
            hover_lines(
                tile,
                pill_hit.as_deref(),
                hovered_top,
                focus_size,
                root_size,
            )
        });
        let target = pill_hit
            .clone()
            .or_else(|| hovered_tile.map(|t| t.path.clone()));
        let target_merged = pill_hit.is_none() && hovered_tile.is_some_and(|t| t.merged);
        let response = match tip {
            Some(lines) => response.on_hover_ui_at_pointer(|ui| {
                ui.set_max_width(360.0);
                for (i, line) in lines.into_iter().enumerate() {
                    if i == 0 {
                        ui.label(egui::RichText::new(line).strong());
                    } else {
                        ui.label(line);
                    }
                }
            }),
            None => response,
        };

        if response.clicked() || response.double_clicked() {
            if let Some(path) = target {
                let is_dir = self
                    .result
                    .as_ref()
                    .and_then(|result| find_node(&result.root, &path))
                    .is_some_and(|node| node.is_dir);
                if let Some(result) = &self.result {
                    expand_to(&result.root, &path, &mut self.expanded);
                }
                self.selected = Some(path.clone());
                if response.double_clicked() && is_dir && !target_merged {
                    self.zoom = Some(path);
                    self.map_cache = None;
                } else if response.double_clicked() && !is_dir {
                    self.reveal_selected();
                }
            }
        }
    }

    fn map_focus<'a>(&self, root: &'a Node) -> &'a Node {
        let Some(path) = &self.zoom else {
            return root;
        };
        find_node(root, path)
            .filter(|node| node.is_dir)
            .unwrap_or(root)
    }

    fn zoom_out(&mut self) {
        let Some(result) = &self.result else {
            self.zoom = None;
            return;
        };
        let Some(path) = self.zoom.clone() else {
            return;
        };
        let parent = find_with_parent_path(&result.root, &path);
        self.zoom = parent.filter(|p| p != &result.root.path);
        self.map_cache = None;
    }
}

fn flatten<'a>(
    node: &'a Node,
    depth: usize,
    expanded: &HashSet<PathBuf>,
    out: &mut Vec<(&'a Node, usize)>,
) {
    out.push((node, depth));
    if node.is_dir && expanded.contains(&node.path) {
        for child in &node.children {
            flatten(child, depth + 1, expanded, out);
        }
    }
}

fn default_path() -> String {
    std::env::var("HOME").unwrap_or_else(|_| String::from("/"))
}

fn format_percent(p: f64) -> String {
    if !p.is_finite() {
        return "—".into();
    }
    if p == 0.0 {
        "0%".into()
    } else if p.abs() >= 0.0001 {
        format!("{p:.4}%")
    } else {
        format!("{p:.2e}%")
    }
}

fn proportion_bar(ui: &mut egui::Ui, size: u64, root_size: u64, key: &str) {
    let width = ui.available_width().max(4.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 12.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 1.0, Color32::from_gray(55));
    let fill_w = share_px(size, root_size, width.floor() as u32) as f32;
    if fill_w > 0.0 {
        let mut fill = rect;
        fill.set_width(fill_w.min(rect.width()));
        let rgb = ext_color(key);
        painter.rect_filled(fill, 1.0, Color32::from_rgb(rgb.r, rgb.g, rgb.b));
    }
}

fn px_from_rect(r: egui::Rect) -> PxRect {
    let x = r.min.x.floor() as i32;
    let y = r.min.y.floor() as i32;
    let x2 = r.max.x.floor() as i32;
    let y2 = r.max.y.floor() as i32;
    PxRect {
        x,
        y,
        w: (x2 - x).max(0) as u32,
        h: (y2 - y).max(0) as u32,
    }
}

fn screen_rect(r: PxRect) -> egui::Rect {
    egui::Rect::from_min_max(
        Pos2::new(r.x as f32, r.y as f32),
        Pos2::new(r.x as f32 + r.w as f32, r.y as f32 + r.h as f32),
    )
}

const MAP_BG: Color32 = Color32::from_rgb(14, 16, 22);
const FRAME_LINE: Color32 = Color32::from_rgb(8, 9, 12);
const PILL_BG: Color32 = Color32::from_rgba_premultiplied(10, 12, 18, 200);
const PILL_HOT: Color32 = Color32::from_rgba_premultiplied(40, 46, 62, 230);
const SELECT: Color32 = Color32::from_rgb(255, 196, 64);

/// Merged small items: slate with a fine hatch so it never reads as a file.
fn paint_other(painter: &egui::Painter, rect: egui::Rect, weight: u64) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(52, 56, 66));
    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(78, 84, 98));
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom()),
                Pos2::new(x + rect.height(), rect.top()),
            ],
            stroke,
        );
        x += 6.0;
    }
    if rect.width() > 64.0 && rect.height() > 34.0 {
        painter.text(
            rect.center() - egui::vec2(0.0, 7.0),
            Align2::CENTER_CENTER,
            "Other",
            FontId::proportional(12.0),
            Color32::WHITE,
        );
        painter.text(
            rect.center() + egui::vec2(0.0, 8.0),
            Align2::CENTER_CENTER,
            format_bytes(weight),
            FontId::monospace(11.0),
            Color32::from_gray(200),
        );
    } else if rect.width() > 36.0 && rect.height() > 16.0 {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Other",
            FontId::proportional(11.0),
            Color32::WHITE,
        );
    }
}

/// Cushion lit from the top left: bright corner, darker opposite corner.
fn paint_cushion(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), mix(color, Color32::WHITE, 0.16));
    mesh.colored_vertex(rect.left_top(), mix(color, Color32::WHITE, 0.30));
    mesh.colored_vertex(rect.right_top(), color);
    mesh.colored_vertex(rect.right_bottom(), mix(color, Color32::BLACK, 0.30));
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    mesh.add_triangle(0, 3, 4);
    mesh.add_triangle(0, 4, 1);
    painter.add(egui::Shape::mesh(mesh));
}

fn mix(color: Color32, toward: Color32, t: f32) -> Color32 {
    let m = |a: u8, b: u8| -> u8 { (f32::from(a) * (1.0 - t) + f32::from(b) * t).round() as u8 };
    Color32::from_rgb(
        m(color.r(), toward.r()),
        m(color.g(), toward.g()),
        m(color.b(), toward.b()),
    )
}

fn luminance(r: u8, g: u8, b: u8) -> f32 {
    0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
}

fn one_line(
    painter: &egui::Painter,
    text: String,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text, font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: max_width.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Tooltip text for the tile under the pointer, or for the folder whose label
/// is under it.
fn hover_lines(
    tile: &crate::Tile,
    pill: Option<&Path>,
    top: Option<&crate::Frame>,
    focus_size: u64,
    root_size: u64,
) -> Vec<String> {
    if let (Some(path), Some(frame)) = (pill, top) {
        if frame.path == path {
            return vec![
                format!("{}/", file_name(path)),
                format!("{}  ·  allocated", format_bytes(frame.weight)),
                format!(
                    "{} of this view  ·  {} of scan",
                    format_scan_share(frame.weight, focus_size),
                    format_scan_share(frame.weight, root_size)
                ),
                "Double-click to zoom in".into(),
            ];
        }
    }
    let mut lines = Vec::new();
    if tile.merged {
        lines.push("Other: smaller items".into());
        lines.push(format!(
            "{}  ·  allocated, combined",
            format_bytes(tile.weight)
        ));
    } else {
        lines.push(file_name(&tile.path));
        lines.push(format!(
            "{}  ·  {}",
            format_bytes(tile.weight),
            ext_description(&tile.color_ext)
        ));
    }
    lines.push(format!(
        "{} of this view  ·  {} of scan",
        format_scan_share(tile.weight, focus_size),
        format_scan_share(tile.weight, root_size)
    ));
    if let Some(frame) = top {
        lines.push(format!(
            "in {}/  ({})",
            file_name(&frame.path),
            format_bytes(frame.weight)
        ));
    }
    lines
}

/// Root-to-focus path segments for the breadcrumb bar.
fn breadcrumbs(root: &Node, focus: &Path) -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = focus
        .ancestors()
        .take_while(|p| p.starts_with(&root.path) && *p != root.path)
        .map(|p| (p.to_path_buf(), file_name(p)))
        .collect();
    let root_name = if root.name.is_empty() {
        root.path.display().to_string()
    } else {
        root.name.clone()
    };
    out.push((root.path.clone(), root_name));
    out.reverse();
    out
}

fn find_with_parent_path(root: &Node, path: &Path) -> Option<PathBuf> {
    fn walk<'a>(node: &'a Node, parent: Option<&'a Node>, path: &Path) -> Option<&'a Node> {
        if node.path == path {
            return parent;
        }
        if node.is_dir && path.starts_with(&node.path) {
            for child in &node.children {
                if let Some(hit) = walk(child, Some(node), path) {
                    return Some(hit);
                }
            }
        }
        None
    }
    walk(root, None, path).map(|node| node.path.clone())
}

fn find_node<'a>(root: &'a Node, path: &Path) -> Option<&'a Node> {
    if root.path == path {
        return Some(root);
    }
    if root.is_dir && path.starts_with(&root.path) {
        for child in &root.children {
            if let Some(hit) = find_node(child, path) {
                return Some(hit);
            }
        }
    }
    None
}

fn expand_to(root: &Node, path: &Path, expanded: &mut HashSet<PathBuf>) {
    fn walk(node: &Node, path: &Path, expanded: &mut HashSet<PathBuf>) -> bool {
        if node.path == path {
            return true;
        }
        if node.is_dir && path.starts_with(&node.path) {
            for child in &node.children {
                if walk(child, path, expanded) {
                    expanded.insert(node.path.clone());
                    return true;
                }
            }
        }
        false
    }
    walk(root, path, expanded);
}

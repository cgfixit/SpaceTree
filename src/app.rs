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
    ext_color, ext_description, ext_label, format_scan_share, layout_node, legend_of, share_px,
    sort_tree, LegendRow, Node, PxRect, ScanResult, SortColumn, Tiling,
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
                        ui.label(format!("walkable {}", human_size(result.root.size)))
                            .on_hover_text(format!("{} bytes", result.root.size));
                        ui.separator();
                        ui.label(format!("volume {}", human_size(result.volume_total)))
                            .on_hover_text(format!("{} bytes", result.volume_total));
                        ui.separator();
                        ui.label(format_percent(result.root.percent_of_disk));
                        if let Some(sel) = &self.selected {
                            ui.separator();
                            if let Some(node) = find_node(&result.root, sel) {
                                ui.label(format!("selected {}", human_size(node.size)))
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
                        " ▾"
                    } else {
                        " ▴"
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
                                        let tri = if open { "▾" } else { "▸" };
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
                                            ui.monospace(human_size(node.size))
                                                .on_hover_text(format!("{} bytes", node.size));
                                        },
                                    );
                                });
                                row.col(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.monospace(human_size(node.logical))
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

    fn legend_panel(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Extension").strong());
        ScrollArea::vertical()
            .id_salt("ext-legend")
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                for row in &self.legend {
                    ui.horizontal(|ui| {
                        let (swatch, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                        let rgb = ext_color(&row.key);
                        ui.painter().rect_filled(
                            swatch,
                            2.0,
                            Color32::from_rgb(rgb.r, rgb.g, rgb.b),
                        );
                        ui.add(Label::new(ext_label(&row.key)).truncate());
                    });
                    ui.add(Label::new(ext_description(&row.key)).truncate());
                    ui.add_space(4.0);
                }
            });
    }

    fn treemap(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        let bounds = px_from_rect(rect);
        let rebuilt = {
            let Some(result) = &self.result else {
                return;
            };
            let focus = self.map_focus(&result.root);
            let stale = self.map_cache.as_ref().is_none_or(|cache| {
                cache.generation != self.generation
                    || cache.focus != focus.path
                    || cache.bounds != bounds
            });
            if stale {
                Some((focus.path.clone(), layout_node(focus, bounds)))
            } else {
                None
            }
        };
        if let Some((focus, tiling)) = rebuilt {
            self.map_cache = Some(MapCache {
                generation: self.generation,
                focus,
                bounds,
                tiling,
            });
        }
        let paints: Vec<(PxRect, Color32, bool, bool)> = {
            let Some(cache) = &self.map_cache else {
                return;
            };
            cache
                .tiling
                .tiles()
                .iter()
                .map(|tile| {
                    let rgb = ext_color(&tile.color_ext);
                    let selected = self.selected.as_deref() == Some(tile.path.as_path());
                    (
                        tile.rect,
                        Color32::from_rgb(rgb.r, rgb.g, rgb.b),
                        selected,
                        tile.merged,
                    )
                })
                .collect()
        };
        let painter = ui.painter().with_clip_rect(rect);
        for (tile, color, selected, merged) in &paints {
            let full = screen_rect(*tile);
            let paint = if tile.w > 2 && tile.h > 2 {
                full.shrink(1.0)
            } else {
                full
            };
            if *merged {
                paint_other(&painter, paint);
            } else if paint.width() >= 8.0 && paint.height() >= 8.0 {
                paint_cushion(&painter, paint, *color);
            } else if paint.width() > 0.0 && paint.height() > 0.0 {
                painter.rect_filled(paint, 0.0, *color);
            }
            painter.rect_stroke(
                full,
                0.0,
                Stroke::new(1.0_f32, Color32::from_rgb(18, 18, 18)),
                egui::StrokeKind::Inside,
            );
            if *selected {
                painter.rect_stroke(
                    full,
                    0.0,
                    Stroke::new(2.0_f32, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
            }
        }
        let zoom_rect = egui::Rect::from_min_size(
            rect.left_top() + egui::vec2(8.0, 8.0),
            egui::vec2(96.0, 24.0),
        );
        let mut zoom_clicked = false;
        if self.zoom.is_some() {
            let btn = ui.put(zoom_rect, egui::Button::new("Zoom out"));
            zoom_clicked = btn.clicked();
        }
        if zoom_clicked {
            self.zoom_out();
            return;
        }
        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        if response.clicked() || response.double_clicked() {
            if let Some(pos) = pointer {
                if self.zoom.is_some() && zoom_rect.contains(pos) {
                    return;
                }
                if let Some(path) = self
                    .map_cache
                    .as_ref()
                    .and_then(|cache| cache.tiling.hit(pos.x.floor() as i32, pos.y.floor() as i32))
                {
                    let path = path.to_path_buf();
                    let is_dir = self
                        .result
                        .as_ref()
                        .and_then(|result| find_node(&result.root, &path))
                        .is_some_and(|node| node.is_dir);
                    if let Some(result) = &self.result {
                        expand_to(&result.root, &path, &mut self.expanded);
                    }
                    self.selected = Some(path.clone());
                    if response.double_clicked() && is_dir && !self.tile_is_merged(&path) {
                        self.zoom = Some(path);
                        self.map_cache = None;
                    } else if response.double_clicked() && !is_dir {
                        self.reveal_selected();
                    }
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

    fn tile_is_merged(&self, path: &std::path::Path) -> bool {
        self.map_cache
            .as_ref()
            .and_then(|cache| {
                cache
                    .tiling
                    .tiles()
                    .iter()
                    .find(|tile| tile.path == path)
                    .map(|tile| tile.merged)
            })
            .unwrap_or(false)
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

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS[u])
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

fn paint_other(painter: &egui::Painter, rect: egui::Rect) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(72, 74, 82));
    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(150, 154, 166));
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom()),
                Pos2::new(x + rect.height(), rect.top()),
            ],
            stroke,
        );
        x += 8.0;
    }
    if rect.width() > 36.0 && rect.height() > 16.0 {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Other",
            FontId::proportional(13.0),
            Color32::WHITE,
        );
    }
}

fn paint_cushion(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let light = mix_white(color, 0.4);
    let mut mesh = Mesh::default();
    let center = rect.center();
    mesh.colored_vertex(center, light);
    mesh.colored_vertex(rect.left_top(), color);
    mesh.colored_vertex(rect.right_top(), color);
    mesh.colored_vertex(rect.right_bottom(), color);
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    mesh.add_triangle(0, 3, 4);
    mesh.add_triangle(0, 4, 1);
    painter.add(egui::Shape::mesh(mesh));
}

fn mix_white(color: Color32, toward: f32) -> Color32 {
    let mix = |channel: u8| -> u8 {
        (f32::from(channel) * (1.0 - toward) + 255.0 * toward).round() as u8
    };
    Color32::from_rgb(mix(color.r()), mix(color.g()), mix(color.b()))
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

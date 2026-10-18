#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod scan;
mod treemap;

use eframe::egui;

use eframe::egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind,
};
use scan::{fmt_bytes, Progress, Tree, CATS, NONE};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ACCENT: Color32 = Color32::from_rgb(59, 130, 196);
const BG: Color32 = Color32::from_rgb(13, 18, 27);
const SURFACE: Color32 = Color32::from_rgb(18, 24, 35);
const LINE: Color32 = Color32::from_rgb(31, 39, 53);
const TEXT: Color32 = Color32::from_rgb(234, 239, 247);
const MUTED: Color32 = Color32::from_rgb(148, 160, 180);
const FAINT: Color32 = Color32::from_rgb(107, 119, 139);
const ROW_H: f32 = 22.0;

fn main() -> eframe::Result {
    let start = std::env::args().nth(1).map(PathBuf::from);
    let mut vp = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 760.0])
        .with_min_inner_size([760.0, 480.0])
        .with_title("OpenStorage");
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")) {
        vp = vp.with_icon(icon);
    }
    eframe::run_native(
        "OpenStorage",
        eframe::NativeOptions { viewport: vp, ..Default::default() },
        Box::new(move |cc| Ok(Box::new(App::new(cc, start)))),
    )
}

struct Scanning {
    prog: Arc<Progress>,
    rx: Receiver<Option<Tree>>,
    since: Instant,
    path: PathBuf,
}

enum Act {
    Select(u32),
    SelectReveal(u32),
    Toggle(u32),
    Zoom(u32),
    Reveal(u32),
    CopyPath(u32),
}

struct App {
    tree: Option<Tree>,
    scanning: Option<Scanning>,
    scan_time: Duration,
    path_input: String,
    expanded: HashSet<u32>,
    selected: Option<u32>,
    view_root: u32,
    rows: Vec<(u32, u16)>,
    rows_dirty: bool,
    scroll_to_sel: bool,
    cells: Vec<treemap::Cell>,
    cells_key: (u32, i32, i32, u32),
    generation: u32,
    hl_cat: Option<u8>,
    quick: Vec<(String, PathBuf)>,
    /// Developer aid: `OPENSTORAGE_SHOT=out.png` saves a screenshot once the scan is shown, then quits.
    shot: Option<(PathBuf, u32)>,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

fn quick_locations() -> Vec<(String, PathBuf)> {
    let mut v = Vec::new();
    if let Some(h) = home() {
        v.push(("Home".to_string(), h.clone()));
        for d in ["Downloads", "Documents", "Desktop"] {
            let p = h.join(d);
            if p.is_dir() {
                v.push((d.to_string(), p));
            }
        }
    }
    #[cfg(windows)]
    for c in b'A'..=b'Z' {
        let p = PathBuf::from(format!("{}:\\", c as char));
        if p.exists() {
            v.push((format!("Drive {}:", c as char), p));
        }
    }
    #[cfg(not(windows))]
    v.push(("Whole disk (/)".to_string(), PathBuf::from("/")));
    v
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        let mut v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = Color32::from_rgb(9, 12, 18);
        v.faint_bg_color = SURFACE;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = ACCENT;
        v.hyperlink_color = Color32::from_rgb(125, 184, 238);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
            w.corner_radius = CornerRadius::same(7);
        }
        v.widgets.inactive.bg_fill = Color32::from_rgb(24, 32, 46);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(24, 32, 46);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(33, 44, 63);
        v.widgets.hovered.bg_fill = Color32::from_rgb(33, 44, 63);
        cc.egui_ctx.set_visuals(v);
        cc.egui_ctx.all_styles_mut(|s| {
            s.spacing.item_spacing = vec2(8.0, 6.0);
            s.spacing.button_padding = vec2(12.0, 5.0);
        });
        let mut app = App {
            tree: None,
            scanning: None,
            scan_time: Duration::ZERO,
            path_input: home().map(|h| h.display().to_string()).unwrap_or_default(),
            expanded: HashSet::new(),
            selected: None,
            view_root: 0,
            rows: Vec::new(),
            rows_dirty: true,
            scroll_to_sel: false,
            cells: Vec::new(),
            cells_key: (NONE, 0, 0, 0),
            generation: 0,
            hl_cat: None,
            quick: quick_locations(),
            shot: std::env::var_os("OPENSTORAGE_SHOT").map(|p| (PathBuf::from(p), 0)),
        };
        if let Some(p) = start {
            app.path_input = p.display().to_string();
            app.start_scan(&cc.egui_ctx, p);
        }
        app
    }

    fn start_scan(&mut self, ctx: &egui::Context, path: PathBuf) {
        if let Some(s) = self.scanning.take() {
            s.prog.cancel.store(true, Relaxed);
        }
        if !path.is_dir() {
            return;
        }
        let prog = Arc::new(Progress::default());
        let (tx, rx) = channel();
        let (p2, ctx2, path2) = (prog.clone(), ctx.clone(), path.clone());
        std::thread::spawn(move || {
            let t = scan::scan(&path2, &p2);
            let _ = tx.send(t);
            ctx2.request_repaint();
        });
        self.scanning = Some(Scanning { prog, rx, since: Instant::now(), path });
    }

    fn finish(&mut self, t: Tree, took: Duration) {
        self.scan_time = took;
        self.expanded.clear();
        self.expanded.insert(0);
        if let Some(&big) = t.nodes[0].children.iter().find(|&&c| t.nodes[c as usize].is_dir) {
            self.expanded.insert(big);
        }
        self.selected = None;
        self.view_root = 0;
        self.hl_cat = None;
        self.tree = Some(t);
        self.rows_dirty = true;
        self.generation += 1;
    }

    fn select(&mut self, id: u32, reveal: bool) {
        self.selected = Some(id);
        if let Some(t) = &self.tree {
            let mut p = t.nodes[id as usize].parent;
            while p != NONE {
                self.expanded.insert(p);
                p = t.nodes[p as usize].parent;
            }
        }
        self.rows_dirty = true;
        self.scroll_to_sel = reveal;
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        let Some(t) = &self.tree else { return };
        let mut stack = vec![(0u32, 0u16)];
        while let Some((id, d)) = stack.pop() {
            self.rows.push((id, d));
            if self.expanded.contains(&id) {
                for &c in t.nodes[id as usize].children.iter().rev() {
                    stack.push((c, d + 1));
                }
            }
        }
        self.rows_dirty = false;
    }

    fn apply(&mut self, ctx: &egui::Context, a: Act) {
        match a {
            Act::Select(id) => self.select(id, false),
            Act::SelectReveal(id) => self.select(id, true),
            Act::Toggle(id) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                }
                self.rows_dirty = true;
            }
            Act::Zoom(id) => {
                if self.tree.as_ref().is_some_and(|t| t.nodes[id as usize].is_dir) {
                    self.view_root = id;
                }
            }
            Act::Reveal(id) => {
                if let Some(t) = &self.tree {
                    reveal(&t.path_of(id), t.nodes[id as usize].is_dir);
                }
            }
            Act::CopyPath(id) => {
                if let Some(t) = &self.tree {
                    ctx.copy_text(t.path_of(id).display().to_string());
                }
            }
        }
    }
}

fn reveal(p: &Path, is_dir: bool) {
    use std::process::Command;
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(if is_dir { "" } else { "-R" }).arg(p).spawn();
    #[cfg(target_os = "windows")]
    let _ = Command::new("explorer").arg(if is_dir { p.as_os_str().to_owned() } else { format!("/select,{}", p.display()).into() }).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(if is_dir { p } else { p.parent().unwrap_or(p) }).spawn();
}

fn cat_color(c: u8) -> Color32 {
    let [r, g, b] = CATS[c as usize].1;
    Color32::from_rgb(r, g, b)
}

fn tint(c: Color32, f: f32) -> Color32 {
    let m = |v: u8| (v as f32 * f).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

fn label(p: &egui::Painter, rect: Rect, text: &str, font: FontId, color: Color32) {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping { max_width: rect.width(), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let g = p.layout_job(job);
    p.galley(rect.left_top(), g, color);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // finished scan
        if let Some(s) = &self.scanning {
            if let Ok(res) = s.rx.try_recv() {
                let took = s.since.elapsed();
                self.scanning = None;
                if let Some(t) = res {
                    self.finish(t, took);
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(80));
            }
        }
        if let Some((path, frames)) = &mut self.shot {
            if self.tree.is_some() {
                *frames += 1;
                if *frames == 4 {
                    let (path, ctx2) = (path.clone(), ctx.clone());
                    ctx.request_screenshot(move |img| {
                        let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                        let _ = image::save_buffer(&path, &raw, img.width() as u32, img.height() as u32, image::ColorType::Rgba8);
                        ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
                    });
                }
                ctx.request_repaint();
            }
        }
        // dropped folder
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(p) = dropped {
            let p = if p.is_dir() { p } else { p.parent().map(Path::to_path_buf).unwrap_or(p) };
            self.path_input = p.display().to_string();
            self.start_scan(&ctx, p);
        }
        if self.tree.is_some() && ctx.input(|i| i.key_pressed(egui::Key::Backspace)) && !ctx.egui_wants_keyboard_input() {
            if let Some(t) = &self.tree {
                let p = t.nodes[self.view_root as usize].parent;
                if p != NONE {
                    self.view_root = p;
                }
            }
        }

        let mut acts: Vec<Act> = Vec::new();
        let mut scan_req: Option<PathBuf> = None;
        let mut stop = false;

        egui::Panel::top("bar")
            .frame(egui::Frame::new().fill(SURFACE).inner_margin(egui::Margin::symmetric(14, 10)).stroke(Stroke::new(1.0, LINE)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("OpenStorage").size(17.0).strong());
                    ui.add_space(10.0);
                    let busy = self.scanning.is_some();
                    let w = (ui.available_width() - 190.0).max(120.0);
                    let r = ui.add_sized(
                        [w, 30.0],
                        egui::TextEdit::singleline(&mut self.path_input).hint_text("Folder or drive to scan").align(Align2::LEFT_CENTER),
                    );
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if busy {
                        if ui.add_sized([84.0, 30.0], egui::Button::new("Stop")).clicked() {
                            stop = true;
                        }
                    } else if ui.add_sized([84.0, 30.0], egui::Button::new(RichText::new("Scan").color(Color32::WHITE)).fill(ACCENT)).clicked() || enter {
                        scan_req = Some(PathBuf::from(self.path_input.trim()));
                    }
                    if self.tree.is_some() && !busy && ui.add_sized([84.0, 30.0], egui::Button::new("Rescan")).clicked() {
                        scan_req = self.tree.as_ref().map(|t| t.root_path.clone());
                    }
                });
            });

        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(SURFACE).inner_margin(egui::Margin::symmetric(14, 6)).stroke(Stroke::new(1.0, LINE)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(t) = &self.tree {
                        let root = &t.nodes[0];
                        ui.label(RichText::new(format!("{} in {} files", fmt_bytes(root.size), root.files)).color(MUTED));
                        ui.label(RichText::new(format!("scanned in {:.1}s", self.scan_time.as_secs_f32())).color(FAINT));
                        if let Some(sel) = self.selected {
                            ui.separator();
                            let n = &t.nodes[sel as usize];
                            ui.label(RichText::new(t.path_of(sel).display().to_string()).monospace().size(12.0).color(TEXT));
                            ui.label(RichText::new(fmt_bytes(n.size)).color(MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("Copy path").clicked() {
                                    acts.push(Act::CopyPath(sel));
                                }
                                if ui.small_button("Reveal").clicked() {
                                    acts.push(Act::Reveal(sel));
                                }
                                if n.is_dir && ui.small_button("Zoom in").clicked() {
                                    acts.push(Act::Zoom(sel));
                                }
                            });
                        }
                    } else {
                        ui.label(RichText::new("Pick a folder to see where your space went.").color(FAINT));
                    }
                });
            });

        if self.tree.is_some() {
            if self.rows_dirty {
                self.rebuild_rows();
            }
            egui::Panel::left("tree")
                .default_size(430.0)
                .size_range(300.0..=760.0)
                .frame(egui::Frame::new().fill(BG).stroke(Stroke::new(1.0, LINE)))
                .show(ui, |ui| self.tree_view(ui, &mut acts));
        }

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::same(10))).show(ui, |ui| {
            if let Some(s) = &self.scanning {
                scanning_view(ui, s);
            } else if self.tree.is_some() {
                self.map_view(ui, &mut acts);
            } else {
                welcome(ui, &self.quick, &mut scan_req);
            }
        });

        if stop {
            if let Some(s) = self.scanning.take() {
                s.prog.cancel.store(true, Relaxed);
            }
        }
        for a in acts {
            self.apply(&ctx, a);
        }
        if let Some(p) = scan_req {
            self.path_input = p.display().to_string();
            self.start_scan(&ctx, p);
        }
    }
}

fn welcome(ui: &mut egui::Ui, quick: &[(String, PathBuf)], req: &mut Option<PathBuf>) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.28).max(20.0));
        ui.label(RichText::new("See what is eating your disk.").size(30.0).strong());
        ui.add_space(4.0);
        ui.label(RichText::new("Scan a folder or drive, or drop one on this window.").size(15.0).color(MUTED));
        ui.add_space(18.0);
        ui.horizontal_wrapped(|ui| {
            let total: f32 = quick.iter().map(|(n, _)| n.len() as f32 * 8.0 + 40.0).sum();
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            for (name, p) in quick {
                if ui.add(egui::Button::new(name.as_str()).min_size(vec2(0.0, 32.0))).clicked() {
                    *req = Some(p.clone());
                }
            }
        });
    });
}

fn scanning_view(ui: &mut egui::Ui, s: &Scanning) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.3).max(20.0));
        ui.spinner();
        ui.add_space(10.0);
        ui.label(RichText::new(format!("Scanning {}", s.path.display())).size(18.0).strong());
        ui.add_space(4.0);
        let f = s.prog.files.load(Relaxed);
        ui.label(
            RichText::new(format!("{f} files · {} folders · {}", s.prog.dirs.load(Relaxed), fmt_bytes(s.prog.bytes.load(Relaxed))))
                .size(15.0)
                .color(MUTED),
        );
        ui.label(RichText::new(format!("{:.1}s", s.since.elapsed().as_secs_f32())).color(FAINT));
    });
}

impl App {
    fn tree_view(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let Some(tree) = &self.tree else { return };
        // header
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 7)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Name").size(12.0).color(FAINT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (w, t) in [(60.0, "Files"), (54.0, "%"), (76.0, "Size")] {
                        ui.allocate_ui_with_layout(vec2(w, 14.0), Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(t).size(12.0).color(FAINT));
                        });
                    }
                });
            });
        });
        ui.separator();
        let mut area = egui::ScrollArea::vertical().auto_shrink(false);
        if self.scroll_to_sel {
            if let Some(i) = self.rows.iter().position(|r| Some(r.0) == self.selected) {
                area = area.vertical_scroll_offset((i as f32 * (ROW_H + ui.spacing().item_spacing.y) - ui.available_height() / 2.0).max(0.0));
            }
            self.scroll_to_sel = false;
        }
        let rows = &self.rows;
        let (selected, expanded) = (self.selected, &self.expanded);
        area.show_rows(ui, ROW_H, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for &(id, depth) in &rows[range] {
                let n = &tree.nodes[id as usize];
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                let p = ui.painter_at(rect);
                if selected == Some(id) {
                    p.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(59, 130, 196, 70));
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 10));
                }
                let parent_size = if n.parent == NONE { n.size } else { tree.nodes[n.parent as usize].size }.max(1);
                let frac = n.size as f32 / parent_size as f32;
                let col = if n.is_dir { ACCENT } else { cat_color(n.cat) };
                let name_right = rect.right() - 12.0 - 60.0 - 54.0 - 76.0;
                let bar = Rect::from_min_max(pos2(rect.left() + 2.0, rect.top() + 3.0), pos2(rect.left() + 2.0 + (name_right - rect.left()) * frac, rect.bottom() - 3.0));
                p.rect_filled(bar, 3.0, col.gamma_multiply(0.16));
                let x = rect.left() + 8.0 + depth as f32 * 14.0;
                let mid = rect.center().y;
                if n.is_dir && !n.children.is_empty() {
                    let open = expanded.contains(&id);
                    let c = pos2(x + 5.0, mid);
                    let pts = if open {
                        vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
                    } else {
                        vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
                    };
                    p.add(egui::Shape::convex_polygon(pts, MUTED, Stroke::NONE));
                }
                // category swatch
                let sw = Rect::from_center_size(pos2(x + 20.0, mid), vec2(9.0, 9.0));
                p.rect_filled(sw, if n.is_dir { 2.0 } else { 4.5 }, col);
                label(&p, Rect::from_min_max(pos2(x + 31.0, rect.top() + 3.0), pos2(name_right, rect.bottom())), &n.name, FontId::proportional(13.5), TEXT);
                let right = rect.right() - 12.0;
                p.text(pos2(right - 60.0 - 54.0, mid), Align2::RIGHT_CENTER, fmt_bytes(n.size), FontId::proportional(13.0), TEXT);
                p.text(pos2(right - 60.0, mid), Align2::RIGHT_CENTER, format!("{:.1}", frac * 100.0), FontId::proportional(13.0), MUTED);
                p.text(pos2(right, mid), Align2::RIGHT_CENTER, n.files.to_string(), FontId::proportional(13.0), FAINT);

                if resp.clicked() {
                    let on_arrow = resp.interact_pointer_pos().is_some_and(|ip| ip.x < x + 14.0);
                    if n.is_dir && on_arrow {
                        acts.push(Act::Toggle(id));
                    } else {
                        acts.push(Act::Select(id));
                    }
                }
                if resp.double_clicked() {
                    acts.push(if n.is_dir { Act::Toggle(id) } else { Act::Reveal(id) });
                }
                resp.context_menu(|ui| {
                    if n.is_dir && ui.button("Show in map").clicked() {
                        acts.push(Act::Zoom(id));
                        ui.close();
                    }
                    if ui.button("Reveal in file manager").clicked() {
                        acts.push(Act::Reveal(id));
                        ui.close();
                    }
                    if ui.button("Copy path").clicked() {
                        acts.push(Act::CopyPath(id));
                        ui.close();
                    }
                });
            }
        });
    }

    fn map_view(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let Some(tree) = &self.tree else { return };
        // breadcrumb
        let mut crumbs = vec![self.view_root];
        while tree.nodes[*crumbs.last().unwrap() as usize].parent != NONE {
            crumbs.push(tree.nodes[*crumbs.last().unwrap() as usize].parent);
        }
        crumbs.reverse();
        ui.horizontal_wrapped(|ui| {
            for (i, &c) in crumbs.iter().enumerate() {
                if i > 0 {
                    ui.label(RichText::new("›").color(FAINT));
                }
                let name = if i == 0 { tree.root_path.display().to_string() } else { tree.nodes[c as usize].name.clone() };
                let last = i + 1 == crumbs.len();
                if ui.add(egui::Button::new(RichText::new(name).color(if last { TEXT } else { MUTED })).frame(false)).clicked() {
                    acts.push(Act::Zoom(c));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(fmt_bytes(tree.nodes[self.view_root as usize].size)).color(MUTED));
            });
        });

        // legend, reserved at the bottom
        let legend_h = 30.0;
        let map_h = (ui.available_height() - legend_h - 8.0).max(60.0);
        let (resp, painter) = ui.allocate_painter(vec2(ui.available_width(), map_h), Sense::click());
        let rect = resp.rect;
        painter.rect_filled(rect, 6.0, Color32::from_rgb(9, 12, 18));
        let area = rect.shrink(1.0);
        let key = (self.view_root, area.width() as i32, area.height() as i32, self.generation);
        if key != self.cells_key {
            self.cells = treemap::layout(tree, self.view_root, area);
            self.cells_key = key;
        }
        let hover = resp.hover_pos().and_then(|p| treemap::hit(&self.cells, p));
        let dim = self.hl_cat;

        let mut mesh = egui::Mesh::default();
        for c in self.cells.iter() {
            let n = &tree.nodes[c.node as usize];
            if c.framed {
                let d = tree.depth_of(c.node) as f32;
                let fill = if (d as i32) % 2 == 0 { Color32::from_rgb(20, 27, 40) } else { Color32::from_rgb(25, 34, 50) };
                painter.rect_filled(c.rect, 2.0, fill);
                painter.rect_stroke(c.rect, 2.0, Stroke::new(1.0, Color32::from_rgb(38, 49, 69)), StrokeKind::Inside);
                continue;
            }
            let r = c.rect.shrink(0.6);
            if r.width() <= 0.0 || r.height() <= 0.0 {
                continue;
            }
            let mut base = if n.is_dir { Color32::from_rgb(48, 62, 88) } else { cat_color(n.cat) };
            if let Some(h) = dim {
                if n.is_dir || n.cat != h {
                    base = tint(base, 0.28);
                }
            }
            let (a, b) = (tint(base, 1.14), tint(base, 0.80));
            let base_i = mesh.vertices.len() as u32;
            mesh.colored_vertex(r.left_top(), a);
            mesh.colored_vertex(r.right_top(), tint(base, 0.97));
            mesh.colored_vertex(r.right_bottom(), b);
            mesh.colored_vertex(r.left_bottom(), tint(base, 0.97));
            mesh.add_triangle(base_i, base_i + 1, base_i + 2);
            mesh.add_triangle(base_i, base_i + 2, base_i + 3);
        }
        painter.add(egui::Shape::mesh(mesh));

        // labels
        let lp = painter.with_clip_rect(rect);
        for c in &self.cells {
            let n = &tree.nodes[c.node as usize];
            let r = c.rect;
            if c.framed {
                if r.width() > 46.0 && r.height() > 38.0 {
                    label(&lp, Rect::from_min_max(r.left_top() + vec2(4.0, 1.0), pos2(r.right() - 3.0, r.top() + 15.0)), &n.name, FontId::proportional(11.5), MUTED);
                }
            } else if r.width() > 54.0 && r.height() > 18.0 && !n.is_dir {
                let tc = Color32::from_rgba_unmultiplied(8, 12, 20, 230);
                label(&lp, Rect::from_min_max(r.left_top() + vec2(4.0, 2.0), pos2(r.right() - 3.0, r.bottom())), &n.name, FontId::proportional(11.5), tc);
                if r.height() > 34.0 {
                    label(&lp, Rect::from_min_max(r.left_top() + vec2(4.0, 16.0), pos2(r.right() - 3.0, r.bottom())), &fmt_bytes(n.size), FontId::proportional(10.5), tc.gamma_multiply(0.7));
                }
            }
        }

        // selection + hover outlines
        let outline = |node: u32| -> Option<Rect> {
            let mut id = node;
            loop {
                if let Some(c) = self.cells.iter().find(|c| c.node == id) {
                    return Some(c.rect);
                }
                let p = tree.nodes[id as usize].parent;
                if p == NONE {
                    return None;
                }
                id = p;
            }
        };
        if let Some(r) = self.selected.and_then(outline) {
            painter.rect_stroke(r.shrink(0.5), 2.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Inside);
        }
        if let Some(h) = hover {
            let c = &self.cells[h];
            painter.rect_stroke(c.rect.shrink(0.5), 2.0, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
            let n = &tree.nodes[c.node as usize];
            let id = c.node;
            resp.clone().on_hover_ui_at_pointer(|ui| {
                ui.set_max_width(420.0);
                ui.label(RichText::new(&n.name).strong());
                ui.label(RichText::new(tree.path_of(id).display().to_string()).monospace().size(11.0).color(FAINT));
                let share = n.size as f64 * 100.0 / tree.nodes[0].size.max(1) as f64;
                let mut line = format!("{}  ·  {:.2}% of scan", fmt_bytes(n.size), share);
                if n.is_dir {
                    line += &format!("  ·  {} files", n.files);
                } else {
                    line += &format!("  ·  {}", CATS[n.cat as usize].0);
                }
                ui.label(RichText::new(line).color(MUTED));
            });
            if resp.clicked() {
                acts.push(Act::SelectReveal(id));
            }
            if resp.double_clicked() {
                acts.push(if n.is_dir { Act::Zoom(id) } else { Act::Reveal(id) });
            }
        }

        // legend
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let total: u64 = tree.cat_bytes.iter().sum::<u64>().max(1);
            let mut order: Vec<usize> = (0..CATS.len()).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(tree.cat_bytes[i]));
            for i in order.into_iter().filter(|&i| tree.cat_bytes[i] > 0) {
                let text = format!("{}  {}  {:.0}%", CATS[i].0, fmt_bytes(tree.cat_bytes[i]), tree.cat_bytes[i] as f64 * 100.0 / total as f64);
                let g = ui.painter().layout_no_wrap(text, FontId::proportional(12.5), MUTED);
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 30.0, 24.0), Sense::click());
                let on = self.hl_cat == Some(i as u8);
                if on || resp.hovered() {
                    ui.painter().rect_filled(r, 6.0, if on { Color32::from_rgb(33, 44, 63) } else { Color32::from_rgb(24, 32, 46) });
                }
                ui.painter().rect_filled(Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(10.0, 10.0)), 3.0, cat_color(i as u8));
                ui.painter().galley(pos2(r.left() + 24.0, r.center().y - g.size().y / 2.0), g, if on { TEXT } else { MUTED });
                if resp.clicked() {
                    self.hl_cat = if on { None } else { Some(i as u8) };
                }
            }
        });
    }
}

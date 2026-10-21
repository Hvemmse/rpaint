#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use eframe::egui::{
    self, pos2,
    text::{CCursor, LayoutJob, TextFormat},
    Align, Align2, Color32, FontId, Key, Rect, ViewportCommand,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

// ================= CONFIG ================= //
// Samme fil og samme nøgler som Python-versionen (~/.notepad_pro_config.json)

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Config {
    dark_mode: bool,
    font_size: u32,
    fullscreen: bool,
    word_wrap: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dark_mode: false,
            font_size: 12,
            fullscreen: false,
            word_wrap: true,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".notepad_pro_config.json"))
}

impl Config {
    fn load() -> Self {
        config_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        if let Some(p) = config_path() {
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = fs::write(p, s);
            }
        }
    }
}

// ================= HJÆLPERE ================= //

fn char_to_byte(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map(|(i, _)| i).unwrap_or(s.len())
}

/// Find `word` fra char-index `from`, med wrap-around. Returnerer (start, slut) i chars.
fn find_from(text: &str, word: &str, from: usize) -> Option<(usize, usize)> {
    if word.is_empty() {
        return None;
    }
    let from_byte = char_to_byte(text, from);
    let found = text[from_byte..]
        .find(word)
        .map(|b| b + from_byte)
        .or_else(|| text[..from_byte].find(word));
    found.map(|b| {
        let start = text[..b].chars().count();
        (start, start + word.chars().count())
    })
}

// ================= APP ================= //

struct NotepadPro {
    cfg: Config,
    text: String,
    filepath: Option<PathBuf>,
    dirty: bool,

    editor_id: egui::Id,
    cursor_idx: usize,
    highlight: Option<(usize, usize)>,
    scroll_to: Option<usize>,

    show_find: bool,
    show_replace: bool,
    focus_find: bool,
    show_help: bool,
    find_text: String,
    replace_text: String,

    last_title: String,
}

impl NotepadPro {
    fn new(ctx: &egui::Context, cfg: Config, initial: Option<PathBuf>) -> Self {
        ctx.set_visuals(if cfg.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let mut app = Self {
            cfg,
            text: String::new(),
            filepath: None,
            dirty: false,
            editor_id: egui::Id::new("editor"),
            cursor_idx: 0,
            highlight: None,
            scroll_to: None,
            show_find: false,
            show_replace: false,
            focus_find: false,
            show_help: false,
            find_text: String::new(),
            replace_text: String::new(),
            last_title: String::new(),
        };
        if let Some(p) = initial {
            app.load_path(p);
        }
        app
    }

    // ----- file ----- //

    fn new_file(&mut self) {
        self.text.clear();
        self.filepath = None;
        self.dirty = false;
        self.highlight = None;
    }

    fn open_file(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_file() {
            self.load_path(p);
        }
    }

    /// Indlæs en fil fra en given sti (bruges af Open og af kommandolinje-argumentet).
    /// Findes filen ikke, åbnes en tom fil med den sti, så Save opretter den.
    fn load_path(&mut self, p: PathBuf) {
        match fs::read(&p) {
            Ok(bytes) => {
                self.text = String::from_utf8_lossy(&bytes).into_owned();
                self.filepath = Some(p);
                self.dirty = false;
                self.highlight = None;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.text.clear();
                self.filepath = Some(p);
                self.dirty = false;
                self.highlight = None;
            }
            Err(_) => {}
        }
    }

    fn save_file(&mut self) {
        match self.filepath.clone() {
            None => self.save_as(),
            Some(p) => {
                if fs::write(p, &self.text).is_ok() {
                    self.dirty = false;
                }
            }
        }
    }

    fn save_as(&mut self) {
        if let Some(mut p) = rfd::FileDialog::new().save_file() {
            if p.extension().is_none() {
                p.set_extension("txt");
            }
            self.filepath = Some(p);
            self.save_file();
        }
    }

    // ----- find / replace ----- //

    fn find_next(&mut self) {
        let from = self.highlight.map(|h| h.1).unwrap_or(self.cursor_idx);
        self.highlight = find_from(&self.text, &self.find_text, from);
        self.scroll_to = self.highlight.map(|h| h.0);
    }

    fn replace_one(&mut self) {
        if self.find_text.is_empty() {
            return;
        }
        if let Some((s, e)) = self.highlight {
            let bs = char_to_byte(&self.text, s);
            let be = char_to_byte(&self.text, e);
            if self.text[bs..be] == self.find_text {
                self.text.replace_range(bs..be, &self.replace_text);
                self.dirty = true;
                self.cursor_idx = s + self.replace_text.chars().count();
                self.highlight = None;
            }
        }
        self.find_next();
    }

    fn replace_all(&mut self) {
        if self.find_text.is_empty() {
            return;
        }
        self.text = self.text.replace(&self.find_text, &self.replace_text);
        self.dirty = true;
        self.highlight = None;
    }

    // ----- settings ----- //

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.cfg.dark_mode = dark;
        ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        self.cfg.save();
    }

    fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        self.cfg.fullscreen = on;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(on));
        self.cfg.save();
    }

    fn open_find(&mut self) {
        self.show_find = true;
        self.focus_find = true;
    }
}

impl eframe::App for NotepadPro {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ---------- genveje ---------- //
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));

        if cmd && pressed(Key::N) {
            self.new_file();
        }
        if cmd && pressed(Key::O) {
            self.open_file();
        }
        if cmd && !shift && pressed(Key::S) {
            self.save_file();
        }
        if cmd && shift && pressed(Key::S) {
            self.save_as();
        }
        if cmd && pressed(Key::F) {
            self.open_find();
        }
        if cmd && pressed(Key::H) {
            self.show_replace = true;
        }
        if cmd && (pressed(Key::Plus) || pressed(Key::Equals)) {
            self.cfg.font_size += 1;
            self.cfg.save();
        }
        if cmd && pressed(Key::Minus) && self.cfg.font_size > 6 {
            self.cfg.font_size -= 1;
            self.cfg.save();
        }
        if cmd && pressed(Key::Num0) {
            self.cfg.font_size = 12;
            self.cfg.save();
        }
        if pressed(Key::F11) {
            let on = !self.cfg.fullscreen;
            self.set_fullscreen(ctx, on);
        }
        if pressed(Key::F1) {
            self.show_help = !self.show_help;
        }
        if pressed(Key::Escape) && self.show_help {
            self.show_help = false;
        }
        if pressed(Key::Escape) && self.cfg.fullscreen {
            self.set_fullscreen(ctx, false);
        }

        // ---------- vinduestitel ---------- //
        let name = self
            .filepath
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        let title = format!("{}{} - Notepad Pro", name, if self.dirty { "*" } else { "" });
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        // ---------- menu ---------- //
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.new_file();
                        ui.close_menu();
                    }
                    if ui.button("Open").clicked() {
                        ui.close_menu();
                        self.open_file();
                    }
                    if ui.button("Save").clicked() {
                        ui.close_menu();
                        self.save_file();
                    }
                    if ui.button("Save As").clicked() {
                        ui.close_menu();
                        self.save_as();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Find").clicked() {
                        self.open_find();
                        ui.close_menu();
                    }
                    if ui.button("Replace").clicked() {
                        self.show_replace = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Format", |ui| {
                    if ui.checkbox(&mut self.cfg.word_wrap, "Word Wrap").changed() {
                        self.cfg.save();
                    }
                    let mut dark = self.cfg.dark_mode;
                    if ui.checkbox(&mut dark, "Dark Mode").changed() {
                        self.set_dark(ctx, dark);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Fullscreen (F11)").clicked() {
                        let on = !self.cfg.fullscreen;
                        self.set_fullscreen(ctx, on);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Shortcuts (F1)").clicked() {
                        self.show_help = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // ---------- statuslinje ---------- //
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let mut line = 1;
            let mut col = 1;
            for ch in self.text.chars().take(self.cursor_idx) {
                if ch == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(format!("Ln {}, Col {}", line, col));
            });
        });

        // ---------- editor ---------- //
        egui::CentralPanel::default().show(ctx, |ui| {
            let font_id = FontId::monospace(self.cfg.font_size as f32);
            let wrap = self.cfg.word_wrap;
            let hl = self.highlight;
            let text_color = ui.visuals().text_color();
            let avail_h = ui.available_height();

            let glyph_w = ui.fonts(|f| f.glyph_width(&font_id, '0'));
            let digits = (self.text.matches('\n').count() + 1).to_string().len().max(3);
            let gutter_w = glyph_w * digits as f32 + 14.0;

            egui::ScrollArea::new([!wrap, true])
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(gutter_w);

                        let width = if wrap { ui.available_width() } else { f32::INFINITY };
                        let fid = font_id.clone();
                        let mut layouter = move |ui: &egui::Ui, text: &str, wrap_width: f32| {
                            let fmt = TextFormat {
                                font_id: fid.clone(),
                                color: text_color,
                                ..Default::default()
                            };
                            let mut job = LayoutJob::default();
                            match hl {
                                Some((s, e)) if s < e => {
                                    let bs = char_to_byte(text, s);
                                    let be = char_to_byte(text, e);
                                    let mut hf = fmt.clone();
                                    hf.background = Color32::YELLOW;
                                    hf.color = Color32::BLACK;
                                    job.append(&text[..bs], 0.0, fmt.clone());
                                    job.append(&text[bs..be], 0.0, hf);
                                    job.append(&text[be..], 0.0, fmt);
                                }
                                _ => job.append(text, 0.0, fmt),
                            }
                            job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
                            ui.fonts(|f| f.layout_job(job))
                        };

                        let output = egui::TextEdit::multiline(&mut self.text)
                            .id(self.editor_id)
                            .font(font_id.clone())
                            .frame(false)
                            .desired_width(width)
                            .min_size(egui::vec2(0.0, avail_h))
                            .layouter(&mut layouter)
                            .show(ui);

                        if output.response.changed() {
                            self.dirty = true;
                            self.highlight = None;
                        }
                        if let Some(range) = output.cursor_range {
                            self.cursor_idx = range.primary.ccursor.index;
                        }

                        // scroll til fundet match
                        if let Some(idx) = self.scroll_to.take() {
                            let r = output
                                .galley
                                .pos_from_ccursor(CCursor::new(idx))
                                .translate(output.galley_pos.to_vec2());
                            ui.scroll_to_rect(r, Some(Align::Center));
                        }

                        // linjenumre (følger logiske linjer, også ved word wrap)
                        let clip = ui.clip_rect();
                        let gx = clip.left();
                        let painter = ui.painter();
                        painter.rect_filled(
                            Rect::from_min_max(
                                pos2(gx, clip.top()),
                                pos2(gx + gutter_w, clip.bottom()),
                            ),
                            0.0,
                            ui.visuals().panel_fill,
                        );
                        let num_color = ui.visuals().weak_text_color();
                        let mut line_no = 1;
                        let mut new_line = true;
                        for row in &output.galley.rows {
                            if new_line {
                                painter.text(
                                    pos2(
                                        gx + gutter_w - 6.0,
                                        output.galley_pos.y + row.rect.top(),
                                    ),
                                    Align2::RIGHT_TOP,
                                    line_no.to_string(),
                                    font_id.clone(),
                                    num_color,
                                );
                                line_no += 1;
                            }
                            new_line = row.ends_with_newline;
                        }
                    });
                });
        });

        // ---------- find ---------- //
        if self.show_find {
            let mut open = true;
            egui::Window::new("Find")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Find:");
                        let r = ui.text_edit_singleline(&mut self.find_text);
                        if self.focus_find {
                            r.request_focus();
                            self.focus_find = false;
                        }
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            self.find_next();
                            r.request_focus();
                        }
                        if ui.button("Find Next").clicked() {
                            self.find_next();
                        }
                    });
                });
            self.show_find = open;
        }

        // ---------- replace ---------- //
        if self.show_replace {
            let mut open = true;
            egui::Window::new("Replace")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::Grid::new("replace_grid").show(ui, |ui| {
                        ui.label("Find:");
                        ui.text_edit_singleline(&mut self.find_text);
                        ui.end_row();
                        ui.label("Replace:");
                        ui.text_edit_singleline(&mut self.replace_text);
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Find Next").clicked() {
                            self.find_next();
                        }
                        if ui.button("Replace").clicked() {
                            self.replace_one();
                        }
                        if ui.button("Replace All").clicked() {
                            self.replace_all();
                        }
                    });
                });
            self.show_replace = open;
        }

        // ---------- hjælp (F1) ---------- //
        if self.show_help {
            let mut open = true;
            egui::Window::new("Shortcuts")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let rows = [
                        ("Ctrl+N", "New file"),
                        ("Ctrl+O", "Open file"),
                        ("Ctrl+S", "Save"),
                        ("Ctrl+Shift+S", "Save As"),
                        ("Ctrl+F", "Find"),
                        ("Ctrl+H", "Replace"),
                        ("Ctrl+Z", "Undo"),
                        ("Ctrl+X / C / V", "Cut / Copy / Paste"),
                        ("Ctrl+A", "Select all"),
                        ("Ctrl++ / Ctrl+-", "Zoom in / out"),
                        ("Ctrl+0", "Reset zoom"),
                        ("F11", "Toggle fullscreen"),
                        ("Esc", "Leave fullscreen / close this window"),
                        ("F1", "Show / hide this help"),
                    ];
                    egui::Grid::new("help_grid")
                        .num_columns(2)
                        .spacing([24.0, 6.0])
                        .show(ui, |ui| {
                            for (key, desc) in rows {
                                ui.label(egui::RichText::new(key).monospace().strong());
                                ui.label(desc);
                                ui.end_row();
                            }
                        });
                });
            self.show_help = open;
        }
    }
}

// ================= MAIN ================= //

fn main() -> eframe::Result<()> {
    let cfg = Config::load();
    // Første argument = fil der skal åbnes (fx fra Nemo: notepadpro %f)
    let initial: Option<PathBuf> = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .map(|p| {
            // Understøt også file:// URI'er
            match p.to_str().and_then(|s| s.strip_prefix("file://")) {
                Some(rest) => PathBuf::from(rest),
                None => p,
            }
        });
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_fullscreen(cfg.fullscreen)
            .with_title("Untitled - Notepad Pro"),
        ..Default::default()
    };
    eframe::run_native(
        "Notepad Pro",
        options,
        Box::new(move |cc| {
            // Ctrl +/- skal ændre skriftstørrelse, ikke hele UI'et
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(NotepadPro::new(&cc.egui_ctx, cfg, initial)))
        }),
    )
}
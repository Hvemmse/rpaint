// rpaint – en Paint-klon i Rust (eframe/egui 0.29 + image 0.25)
use eframe::egui::{
    self, pos2, vec2, Color32, ColorImage, Key, Modifiers, PointerButton, Pos2, Rect, Sense,
    Stroke, TextureHandle, TextureOptions,
};
use image::{imageops, Rgba, RgbaImage};
use std::path::{Path, PathBuf};

const MAX_DIM: u32 = 8000;
const UNDO_LIMIT: usize = 30;
const ZOOMS: [f32; 12] = [
    0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0,
];
const PALETTE: [(u8, u8, u8); 20] = [
    (0, 0, 0),
    (127, 127, 127),
    (136, 0, 21),
    (237, 28, 36),
    (255, 127, 39),
    (255, 242, 0),
    (34, 177, 76),
    (0, 162, 232),
    (63, 72, 204),
    (163, 73, 164),
    (255, 255, 255),
    (195, 195, 195),
    (185, 122, 87),
    (255, 174, 201),
    (255, 201, 14),
    (239, 228, 176),
    (181, 230, 29),
    (153, 217, 234),
    (112, 146, 190),
    (200, 191, 231),
];

type P = (i32, i32);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Da,
    En,
}

fn tr(l: Lang, da: &'static str, en: &'static str) -> &'static str {
    match l {
        Lang::Da => da,
        Lang::En => en,
    }
}

fn lang_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/rpaint/lang"))
}

fn load_lang() -> Lang {
    if let Some(p) = lang_config_path() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            match s.trim() {
                "da" => return Lang::Da,
                "en" => return Lang::En,
                _ => {}
            }
        }
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                return if v.starts_with("da") { Lang::Da } else { Lang::En };
            }
        }
    }
    Lang::En
}

fn save_lang(l: Lang) {
    if let Some(p) = lang_config_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, if l == Lang::Da { "da" } else { "en" });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Pencil,
    Brush,
    Eraser,
    Fill,
    Picker,
    Spray,
    Line,
    Rect,
    Ellipse,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FillMode {
    Outline,
    Both,
    Filled,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialog {
    None,
    New,
    Open,
    SaveAs,
    Resize,
    Stretch,
    Help,
}

#[derive(Clone, Copy)]
enum ImageOp {
    FlipH,
    FlipV,
    Rot90,
    Rot270,
    Rot180,
    Invert,
    Clear,
}

#[derive(Clone, Copy)]
enum SelDrag {
    New { start: P },
    Move { grab: P, orig: P },
}

struct Sel {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    floating: Option<RgbaImage>,
}

struct Drag {
    last: P,
    start: P,
    secondary: bool,
    base: Option<RgbaImage>,
}

// ---------------------------------------------------------------- tegne-primitiver

fn rgba(c: Color32) -> Rgba<u8> {
    Rgba([c.r(), c.g(), c.b(), 255])
}

fn put(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, c);
    }
}

fn stamp(img: &mut RgbaImage, cx: i32, cy: i32, size: i32, c: Rgba<u8>, round: bool) {
    if size <= 1 {
        put(img, cx, cy, c);
        return;
    }
    let r = size as f32 / 2.0;
    let x0 = cx - size / 2;
    let y0 = cy - size / 2;
    for dy in 0..size {
        for dx in 0..size {
            if round {
                let fx = dx as f32 + 0.5 - r;
                let fy = dy as f32 + 0.5 - r;
                if fx * fx + fy * fy > r * r {
                    continue;
                }
            }
            put(img, x0 + dx, y0 + dy, c);
        }
    }
}

fn draw_line(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, size: i32, c: Rgba<u8>, round: bool) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        stamp(img, x, y, size, c, round);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn fill_rect(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba<u8>) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            img.put_pixel(x as u32, y as u32, c);
        }
    }
}

fn draw_rect(img: &mut RgbaImage, a: P, b: P, size: i32, col: Rgba<u8>, other: Rgba<u8>, mode: FillMode) {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    match mode {
        FillMode::Filled => fill_rect(img, x0, y0, x1, y1, col),
        FillMode::Both => fill_rect(img, x0, y0, x1, y1, other),
        FillMode::Outline => {}
    }
    if mode != FillMode::Filled {
        draw_line(img, x0, y0, x1, y0, size, col, false);
        draw_line(img, x1, y0, x1, y1, size, col, false);
        draw_line(img, x1, y1, x0, y1, size, col, false);
        draw_line(img, x0, y1, x0, y0, size, col, false);
    }
}

fn draw_ellipse(img: &mut RgbaImage, a: P, b: P, size: i32, col: Rgba<u8>, other: Rgba<u8>, mode: FillMode) {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    let left = x0 as f32;
    let right = (x1 + 1) as f32;
    let top = y0 as f32;
    let bot = (y1 + 1) as f32;
    let (cx, cy) = ((left + right) / 2.0, (top + bot) / 2.0);
    let (rx, ry) = ((right - left) / 2.0, (bot - top) / 2.0);
    if mode != FillMode::Outline {
        let fc = if mode == FillMode::Filled { col } else { other };
        for y in y0..=y1 {
            let t = ((y as f32 + 0.5) - cy) / ry;
            if t.abs() > 1.0 {
                continue;
            }
            let w = rx * (1.0 - t * t).sqrt();
            let xs = (cx - w - 0.5).ceil() as i32;
            let xe = (cx + w - 0.5).floor() as i32;
            for x in xs..=xe {
                put(img, x, y, fc);
            }
        }
    }
    if mode != FillMode::Filled {
        let n = (((rx + ry) * 4.0).max(32.0)) as usize;
        let mut prev: Option<P> = None;
        for i in 0..=n {
            let ang = i as f32 / n as f32 * std::f32::consts::TAU;
            let px = (cx + (rx - 0.5).max(0.0) * ang.cos() - 0.5).round() as i32;
            let py = (cy + (ry - 0.5).max(0.0) * ang.sin() - 0.5).round() as i32;
            match prev {
                Some(pv) => draw_line(img, pv.0, pv.1, px, py, size, col, true),
                None => stamp(img, px, py, size, col, true),
            }
            prev = Some((px, py));
        }
    }
}

fn draw_shape(img: &mut RgbaImage, tool: Tool, a: P, b: P, size: i32, col: Rgba<u8>, other: Rgba<u8>, mode: FillMode) {
    match tool {
        Tool::Line => draw_line(img, a.0, a.1, b.0, b.1, size, col, true),
        Tool::Rect => draw_rect(img, a, b, size, col, other, mode),
        Tool::Ellipse => draw_ellipse(img, a, b, size, col, other, mode),
        _ => {}
    }
}

fn constrain(s: P, p: P, line: bool) -> P {
    let (mut dx, mut dy) = (p.0 - s.0, p.1 - s.1);
    let (ax, ay) = (dx.abs(), dy.abs());
    if line && ax > 2 * ay {
        dy = 0;
    } else if line && ay > 2 * ax {
        dx = 0;
    } else {
        let m = ax.max(ay);
        dx = if dx < 0 { -m } else { m };
        dy = if dy < 0 { -m } else { m };
    }
    (s.0 + dx, s.1 + dy)
}

fn next_rand(rng: &mut u64) -> u32 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *rng = x;
    (x >> 32) as u32
}

fn spray(img: &mut RgbaImage, rng: &mut u64, c: P, size: i32, col: Rgba<u8>) {
    let radius = (size * 2).max(4) as f32;
    let n = (size * 3).max(8);
    for _ in 0..n {
        let a = next_rand(rng) as f32 / u32::MAX as f32 * std::f32::consts::TAU;
        let d = (next_rand(rng) as f32 / u32::MAX as f32).sqrt() * radius;
        put(img, c.0 + (a.cos() * d).round() as i32, c.1 + (a.sin() * d).round() as i32, col);
    }
}

fn flood_fill(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() {
        return;
    }
    let target = *img.get_pixel(x as u32, y as u32);
    if target == c {
        return;
    }
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut stack = vec![(x, y)];
    while let Some((sx, sy)) = stack.pop() {
        if *img.get_pixel(sx as u32, sy as u32) != target {
            continue;
        }
        let mut l = sx;
        while l > 0 && *img.get_pixel((l - 1) as u32, sy as u32) == target {
            l -= 1;
        }
        let mut r = sx;
        while r < w - 1 && *img.get_pixel((r + 1) as u32, sy as u32) == target {
            r += 1;
        }
        for xx in l..=r {
            img.put_pixel(xx as u32, sy as u32, c);
        }
        for ny in [sy - 1, sy + 1] {
            if ny < 0 || ny >= h {
                continue;
            }
            let mut in_span = false;
            for xx in l..=r {
                let m = *img.get_pixel(xx as u32, ny as u32) == target;
                if m && !in_span {
                    stack.push((xx, ny));
                    in_span = true;
                } else if !m {
                    in_span = false;
                }
            }
        }
    }
}

fn blit(dst: &mut RgbaImage, src: &RgbaImage, ox: i32, oy: i32) {
    for y in 0..src.height() {
        for x in 0..src.width() {
            put(dst, ox + x as i32, oy + y as i32, *src.get_pixel(x, y));
        }
    }
}

fn white(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]))
}

fn to_color_image(img: &RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

fn is_image_name(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".png") || n.ends_with(".jpg") || n.ends_with(".jpeg") || n.ends_with(".bmp")
}

// ---------------------------------------------------------------- app

struct PaintApp {
    canvas: RgbaImage,
    undo: Vec<RgbaImage>,
    redo: Vec<RgbaImage>,
    tool: Tool,
    fg: Color32,
    bg: Color32,
    size: u32,
    round: bool,
    fill_mode: FillMode,
    zoom: f32,
    show_grid: bool,
    texture: Option<TextureHandle>,
    dirty: bool,
    drag: Option<Drag>,
    sel: Option<Sel>,
    sel_drag: Option<SelDrag>,
    clipboard: Option<RgbaImage>,
    file: Option<String>,
    status: String,
    hover: Option<P>,
    dialog: Dialog,
    path_input: String,
    dlg_dir: PathBuf,
    dlg_w: u32,
    dlg_h: u32,
    rng: u64,
    pending_title: Option<String>,
    lang: Lang,
}

impl PaintApp {
    fn new() -> Self {
        let mut app = Self {
            canvas: white(800, 600),
            undo: Vec::new(),
            redo: Vec::new(),
            tool: Tool::Pencil,
            fg: Color32::BLACK,
            bg: Color32::WHITE,
            size: 3,
            round: true,
            fill_mode: FillMode::Outline,
            zoom: 1.0,
            show_grid: false,
            texture: None,
            dirty: true,
            drag: None,
            sel: None,
            sel_drag: None,
            clipboard: None,
            file: None,
            status: String::new(),
            hover: None,
            dialog: Dialog::None,
            path_input: String::new(),
            dlg_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            dlg_w: 800,
            dlg_h: 600,
            rng: 0x2545F4914F6CDD1D,
            pending_title: None,
            lang: load_lang(),
        };
        app.status = tr(app.lang, "Klar", "Ready").to_string();
        app.set_title();
        app
    }

    fn set_title(&mut self) {
        let name = self
            .file
            .as_ref()
            .and_then(|f| Path::new(f).file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| tr(self.lang, "Unavngivet", "Untitled").to_string());
        self.pending_title = Some(format!("{} - Paint", name));
    }

    fn colors(&self, secondary: bool) -> (Rgba<u8>, Rgba<u8>) {
        let (a, b) = if secondary { (self.bg, self.fg) } else { (self.fg, self.bg) };
        (rgba(a), rgba(b))
    }

    // ---------- undo / redo

    fn push_undo(&mut self) {
        self.undo.push(self.canvas.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) {
        self.commit_sel();
        self.drag = None;
        self.sel_drag = None;
        if let Some(prev) = self.undo.pop() {
            let cur = std::mem::replace(&mut self.canvas, prev);
            self.redo.push(cur);
            self.sel = None;
            self.dirty = true;
        }
    }

    fn redo(&mut self) {
        self.commit_sel();
        self.drag = None;
        self.sel_drag = None;
        if let Some(next) = self.redo.pop() {
            let cur = std::mem::replace(&mut self.canvas, next);
            self.undo.push(cur);
            self.sel = None;
            self.dirty = true;
        }
    }

    // ---------- markering

    fn commit_sel(&mut self) {
        if let Some(s) = self.sel.take() {
            if let Some(f) = s.floating {
                blit(&mut self.canvas, &f, s.x, s.y);
            }
            self.dirty = true;
        }
    }

    fn select_all(&mut self) {
        self.commit_sel();
        self.tool = Tool::Select;
        self.sel = Some(Sel {
            x: 0,
            y: 0,
            w: self.canvas.width() as i32,
            h: self.canvas.height() as i32,
            floating: None,
        });
    }

    fn copy_sel(&mut self, ctx: &egui::Context) {
        let img = match &self.sel {
            Some(s) => match &s.floating {
                Some(f) => f.clone(),
                None => imageops::crop_imm(&self.canvas, s.x as u32, s.y as u32, s.w as u32, s.h as u32).to_image(),
            },
            None => return,
        };
        self.clipboard = Some(img);
        // Gør systemets clipboard ikke-tomt, så Ctrl+V-events bliver sendt af egui
        ctx.copy_text("rpaint".to_string());
        self.status = tr(self.lang, "Kopieret", "Copied").into();
    }

    fn delete_sel(&mut self) {
        let Some(s) = self.sel.take() else { return };
        if s.floating.is_none() {
            self.push_undo();
            let bg = rgba(self.bg);
            fill_rect(&mut self.canvas, s.x, s.y, s.x + s.w - 1, s.y + s.h - 1, bg);
        }
        self.dirty = true;
    }

    fn paste(&mut self) {
        let Some(img) = self.clipboard.clone() else { return };
        self.commit_sel();
        self.push_undo();
        self.tool = Tool::Select;
        self.sel = Some(Sel {
            x: 0,
            y: 0,
            w: img.width() as i32,
            h: img.height() as i32,
            floating: Some(img),
        });
        self.dirty = true;
    }

    fn crop_to_sel(&mut self) {
        let Some((x, y, w, h)) = self.sel.as_ref().map(|s| (s.x, s.y, s.w, s.h)) else { return };
        self.commit_sel();
        self.push_undo();
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = ((x + w).min(self.canvas.width() as i32), (y + h).min(self.canvas.height() as i32));
        if x1 > x0 && y1 > y0 {
            self.canvas = imageops::crop_imm(&self.canvas, x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32).to_image();
        }
        self.dirty = true;
    }

    fn select_begin(&mut self, p: P) {
        let hit = self.sel.as_ref().map(|s| {
            (
                p.0 >= s.x && p.0 < s.x + s.w && p.1 >= s.y && p.1 < s.y + s.h,
                s.floating.is_none(),
                s.x,
                s.y,
                s.w,
                s.h,
            )
        });
        if let Some((inside, needs_lift, sx, sy, sw, sh)) = hit {
            if inside {
                if needs_lift {
                    self.push_undo();
                    let img = imageops::crop_imm(&self.canvas, sx as u32, sy as u32, sw as u32, sh as u32).to_image();
                    let bg = rgba(self.bg);
                    fill_rect(&mut self.canvas, sx, sy, sx + sw - 1, sy + sh - 1, bg);
                    if let Some(s) = self.sel.as_mut() {
                        s.floating = Some(img);
                    }
                }
                self.sel_drag = Some(SelDrag::Move { grab: p, orig: (sx, sy) });
                self.dirty = true;
                return;
            }
        }
        self.commit_sel();
        self.sel_drag = Some(SelDrag::New { start: p });
    }

    fn select_move(&mut self, p: P) {
        match self.sel_drag {
            Some(SelDrag::New { start }) => {
                let (cw, ch) = (self.canvas.width() as i32, self.canvas.height() as i32);
                let x0 = start.0.min(p.0).clamp(0, cw - 1);
                let x1 = start.0.max(p.0).clamp(0, cw - 1);
                let y0 = start.1.min(p.1).clamp(0, ch - 1);
                let y1 = start.1.max(p.1).clamp(0, ch - 1);
                self.sel = Some(Sel { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1, floating: None });
            }
            Some(SelDrag::Move { grab, orig }) => {
                if let Some(s) = self.sel.as_mut() {
                    s.x = orig.0 + p.0 - grab.0;
                    s.y = orig.1 + p.1 - grab.1;
                }
                self.dirty = true;
            }
            None => {}
        }
    }

    // ---------- tegning

    fn begin(&mut self, p: P, secondary: bool, shift: bool) {
        if self.tool == Tool::Select {
            self.select_begin(p);
            return;
        }
        self.commit_sel();
        let (col, _other) = self.colors(secondary);
        match self.tool {
            Tool::Picker => {
                if p.0 >= 0 && p.1 >= 0 {
                    if let Some(c) = self.canvas.get_pixel_checked(p.0 as u32, p.1 as u32) {
                        let c32 = Color32::from_rgb(c[0], c[1], c[2]);
                        if secondary {
                            self.bg = c32;
                        } else {
                            self.fg = c32;
                        }
                    }
                }
            }
            Tool::Fill => {
                self.push_undo();
                flood_fill(&mut self.canvas, p.0, p.1, col);
                self.dirty = true;
            }
            Tool::Pencil | Tool::Brush | Tool::Eraser | Tool::Spray => {
                self.push_undo();
                self.drag = Some(Drag { last: p, start: p, secondary, base: None });
                self.paint_freehand(p, p, secondary);
            }
            Tool::Line | Tool::Rect | Tool::Ellipse => {
                self.push_undo();
                let base = self.canvas.clone();
                self.drag = Some(Drag { last: p, start: p, secondary, base: Some(base) });
                self.drag_shape(p, shift);
            }
            Tool::Select => {}
        }
    }

    fn paint_freehand(&mut self, a: P, b: P, secondary: bool) {
        let (col, _) = self.colors(secondary);
        let size = self.size as i32;
        match self.tool {
            Tool::Pencil => draw_line(&mut self.canvas, a.0, a.1, b.0, b.1, 1, col, false),
            Tool::Brush => draw_line(&mut self.canvas, a.0, a.1, b.0, b.1, size, col, self.round),
            Tool::Eraser => {
                let bg = rgba(self.bg);
                draw_line(&mut self.canvas, a.0, a.1, b.0, b.1, size.max(2), bg, false)
            }
            Tool::Spray => spray(&mut self.canvas, &mut self.rng, b, size, col),
            _ => {}
        }
        self.dirty = true;
    }

    fn drag_shape(&mut self, p: P, shift: bool) {
        let (start, sec, base) = match &self.drag {
            Some(d) => match &d.base {
                Some(b) => (d.start, d.secondary, b.clone()),
                None => return,
            },
            None => return,
        };
        self.canvas = base;
        let q = if shift { constrain(start, p, self.tool == Tool::Line) } else { p };
        let (col, other) = self.colors(sec);
        draw_shape(&mut self.canvas, self.tool, start, q, self.size as i32, col, other, self.fill_mode);
        self.dirty = true;
    }

    fn drag_move(&mut self, p: P, shift: bool) {
        if self.tool == Tool::Select {
            self.select_move(p);
            return;
        }
        let (last, sec) = match &self.drag {
            Some(d) => (d.last, d.secondary),
            None => return,
        };
        match self.tool {
            Tool::Pencil | Tool::Brush | Tool::Eraser | Tool::Spray => {
                if p != last || self.tool == Tool::Spray {
                    self.paint_freehand(last, p, sec);
                    if let Some(d) = self.drag.as_mut() {
                        d.last = p;
                    }
                }
            }
            Tool::Line | Tool::Rect | Tool::Ellipse => {
                if p != last {
                    self.drag_shape(p, shift);
                    if let Some(d) = self.drag.as_mut() {
                        d.last = p;
                    }
                }
            }
            _ => {}
        }
    }

    fn end(&mut self) {
        if matches!(self.sel_drag, Some(SelDrag::New { .. })) {
            let tiny = self.sel.as_ref().map_or(false, |s| s.w <= 1 && s.h <= 1);
            if tiny {
                self.sel = None;
            }
        }
        self.drag = None;
        self.sel_drag = None;
    }

    // ---------- billedoperationer

    fn image_op(&mut self, op: ImageOp) {
        self.commit_sel();
        self.push_undo();
        let bg = rgba(self.bg);
        self.canvas = match op {
            ImageOp::FlipH => imageops::flip_horizontal(&self.canvas),
            ImageOp::FlipV => imageops::flip_vertical(&self.canvas),
            ImageOp::Rot90 => imageops::rotate90(&self.canvas),
            ImageOp::Rot270 => imageops::rotate270(&self.canvas),
            ImageOp::Rot180 => imageops::rotate180(&self.canvas),
            ImageOp::Invert => {
                let mut c = self.canvas.clone();
                imageops::invert(&mut c);
                c
            }
            ImageOp::Clear => RgbaImage::from_pixel(self.canvas.width(), self.canvas.height(), bg),
        };
        self.dirty = true;
    }

    fn resize_canvas(&mut self, w: u32, h: u32) {
        self.commit_sel();
        self.push_undo();
        let mut n = RgbaImage::from_pixel(w, h, rgba(self.bg));
        blit(&mut n, &self.canvas, 0, 0);
        self.canvas = n;
        self.dirty = true;
    }

    fn stretch(&mut self, w: u32, h: u32) {
        self.commit_sel();
        self.push_undo();
        self.canvas = imageops::resize(&self.canvas, w, h, imageops::FilterType::Triangle);
        self.dirty = true;
    }

    fn new_image(&mut self, w: u32, h: u32) {
        self.canvas = white(w, h);
        self.undo.clear();
        self.redo.clear();
        self.sel = None;
        self.drag = None;
        self.sel_drag = None;
        self.file = None;
        self.dirty = true;
        self.set_title();
    }

    // ---------- filer

    fn open_file(&mut self, path: &Path) -> Result<(), String> {
        let mut img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
        if img.width() > MAX_DIM || img.height() > MAX_DIM {
            return Err(format!("{} {}x{}", tr(self.lang, "Billedet er for stort, maks", "Image too large, max"), MAX_DIM, MAX_DIM));
        }
        // Læg gennemsigtighed over hvid
        for px in img.pixels_mut() {
            let a = px[3] as u32;
            if a < 255 {
                for i in 0..3 {
                    px[i] = ((px[i] as u32 * a + 255 * (255 - a)) / 255) as u8;
                }
                px[3] = 255;
            }
        }
        self.canvas = img;
        self.undo.clear();
        self.redo.clear();
        self.sel = None;
        self.drag = None;
        self.sel_drag = None;
        self.dirty = true;
        self.file = Some(path.to_string_lossy().to_string());
        self.set_title();
        Ok(())
    }

    fn save_to(&mut self, path: &Path) -> Result<(), String> {
        self.commit_sel();
        let mut p = path.to_path_buf();
        if p.extension().is_none() {
            p.set_extension("png");
        }
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let res = match ext.as_str() {
            "jpg" | "jpeg" => image::DynamicImage::ImageRgba8(self.canvas.clone()).to_rgb8().save(&p),
            _ => self.canvas.save(&p),
        };
        res.map_err(|e| e.to_string())?;
        self.file = Some(p.to_string_lossy().to_string());
        self.set_title();
        Ok(())
    }

    fn quick_save(&mut self) {
        match self.file.clone() {
            Some(f) => match self.save_to(Path::new(&f)) {
                Ok(()) => self.status = format!("{}: {}", tr(self.lang, "Gemt", "Saved"), f),
                Err(e) => self.status = format!("{}: {}", tr(self.lang, "Fejl ved gem", "Save failed"), e),
            },
            None => self.open_dialog(Dialog::SaveAs),
        }
    }

    fn open_dialog(&mut self, d: Dialog) {
        match d {
            Dialog::New => {
                self.dlg_w = 800;
                self.dlg_h = 600;
            }
            Dialog::Resize | Dialog::Stretch => {
                self.dlg_w = self.canvas.width();
                self.dlg_h = self.canvas.height();
            }
            Dialog::SaveAs => {
                self.path_input = self
                    .file
                    .as_ref()
                    .and_then(|f| Path::new(f).file_name().map(|n| n.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "untitled.png".to_string());
            }
            Dialog::Open => self.path_input.clear(),
            Dialog::None | Dialog::Help => {}
        }
        self.dialog = d;
    }

    // ---------- visning

    fn zoom_step(&mut self, dir: i32) {
        let idx = ZOOMS.iter().position(|&z| (z - self.zoom).abs() < 0.01).unwrap_or(2) as i32;
        let n = (idx + dir).clamp(0, ZOOMS.len() as i32 - 1);
        self.zoom = ZOOMS[n as usize];
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        if !self.dirty && self.texture.is_some() {
            return;
        }
        let ci = match &self.sel {
            Some(Sel { x, y, floating: Some(f), .. }) => {
                let mut tmp = self.canvas.clone();
                blit(&mut tmp, f, *x, *y);
                to_color_image(&tmp)
            }
            _ => to_color_image(&self.canvas),
        };
        match &mut self.texture {
            Some(t) => t.set(ci, TextureOptions::NEAREST),
            None => self.texture = Some(ctx.load_texture("canvas", ci, TextureOptions::NEAREST)),
        }
        self.dirty = false;
    }

    // ---------- UI

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) {
            self.redo();
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::Z)) {
            self.undo();
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::Y)) {
            self.redo();
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::N)) {
            self.open_dialog(Dialog::New);
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::O)) {
            self.open_dialog(Dialog::Open);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
            self.open_dialog(Dialog::SaveAs);
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::S)) {
            self.quick_save();
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::A)) {
            self.select_all();
        }
        let (mut c, mut x, mut v) = (false, false, false);
        ctx.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Copy => c = true,
                    egui::Event::Cut => x = true,
                    egui::Event::Paste(_) => v = true,
                    egui::Event::Key { key, pressed: true, modifiers, .. } if modifiers.command => match key {
                        Key::C => c = true,
                        Key::X => x = true,
                        Key::V => v = true,
                        _ => {}
                    },
                    _ => {}
                }
            }
        });
        if c || x {
            self.copy_sel(ctx);
        }
        if x {
            self.delete_sel();
        }
        if v {
            self.paste();
        }
        if ctx.input(|i| i.key_pressed(Key::Delete)) {
            self.delete_sel();
        }
        if ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.commit_sel();
        }
        let zd = ctx.input(|i| i.zoom_delta());
        if zd > 1.001 {
            self.zoom_step(1);
        } else if zd < 0.999 {
            self.zoom_step(-1);
        }
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(tr(l, "Fil", "File"), |ui| {
                    if ui.add(egui::Button::new(tr(l, "Ny…", "New…")).shortcut_text("Ctrl+N")).clicked() {
                        self.open_dialog(Dialog::New);
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(tr(l, "Åbn…", "Open…")).shortcut_text("Ctrl+O")).clicked() {
                        self.open_dialog(Dialog::Open);
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(tr(l, "Gem", "Save")).shortcut_text("Ctrl+S")).clicked() {
                        self.quick_save();
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(tr(l, "Gem som…", "Save as…")).shortcut_text("Ctrl+Shift+S")).clicked() {
                        self.open_dialog(Dialog::SaveAs);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(tr(l, "Afslut", "Exit")).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button(tr(l, "Rediger", "Edit"), |ui| {
                    if ui.add(egui::Button::new(tr(l, "Fortryd", "Undo")).shortcut_text("Ctrl+Z")).clicked() {
                        self.undo();
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(tr(l, "Gentag", "Redo")).shortcut_text("Ctrl+Y")).clicked() {
                        self.redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    let has_sel = self.sel.is_some();
                    if ui.add_enabled(has_sel, egui::Button::new(tr(l, "Klip", "Cut")).shortcut_text("Ctrl+X")).clicked() {
                        self.copy_sel(ui.ctx());
                        self.delete_sel();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_sel, egui::Button::new(tr(l, "Kopiér", "Copy")).shortcut_text("Ctrl+C")).clicked() {
                        self.copy_sel(ui.ctx());
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.clipboard.is_some(), egui::Button::new(tr(l, "Sæt ind", "Paste")).shortcut_text("Ctrl+V")).clicked() {
                        self.paste();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_sel, egui::Button::new(tr(l, "Slet markering", "Delete selection")).shortcut_text("Delete")).clicked() {
                        self.delete_sel();
                        ui.close_menu();
                    }
                    if ui.add(egui::Button::new(tr(l, "Markér alt", "Select all")).shortcut_text("Ctrl+A")).clicked() {
                        self.select_all();
                        ui.close_menu();
                    }
                });
                ui.menu_button(tr(l, "Billede", "Image"), |ui| {
                    if ui.add_enabled(self.sel.is_some(), egui::Button::new(tr(l, "Beskær til markering", "Crop to selection"))).clicked() {
                        self.crop_to_sel();
                        ui.close_menu();
                    }
                    if ui.button(tr(l, "Ændr lærredets størrelse…", "Resize canvas…")).clicked() {
                        self.open_dialog(Dialog::Resize);
                        ui.close_menu();
                    }
                    if ui.button(tr(l, "Skalér/stræk billede…", "Scale/stretch image…")).clicked() {
                        self.open_dialog(Dialog::Stretch);
                        ui.close_menu();
                    }
                    ui.separator();
                    for (label, op) in [
                        (tr(l, "Vend vandret", "Flip horizontal"), ImageOp::FlipH),
                        (tr(l, "Vend lodret", "Flip vertical"), ImageOp::FlipV),
                        (tr(l, "Roter 90° højre", "Rotate 90° right"), ImageOp::Rot90),
                        (tr(l, "Roter 90° venstre", "Rotate 90° left"), ImageOp::Rot270),
                        (tr(l, "Roter 180°", "Rotate 180°"), ImageOp::Rot180),
                        (tr(l, "Invertér farver", "Invert colors"), ImageOp::Invert),
                        (tr(l, "Ryd billede", "Clear image"), ImageOp::Clear),
                    ] {
                        if ui.button(label).clicked() {
                            self.image_op(op);
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button(tr(l, "Vis", "View"), |ui| {
                    if ui.add(egui::Button::new(tr(l, "Zoom ind", "Zoom in")).shortcut_text(tr(l, "Ctrl+Scroll op", "Ctrl+Scroll up"))).clicked() {
                        self.zoom_step(1);
                    }
                    if ui.add(egui::Button::new(tr(l, "Zoom ud", "Zoom out")).shortcut_text(tr(l, "Ctrl+Scroll ned", "Ctrl+Scroll down"))).clicked() {
                        self.zoom_step(-1);
                    }
                    if ui.button("100 %").clicked() {
                        self.zoom = 1.0;
                        ui.close_menu();
                    }
                    ui.checkbox(&mut self.show_grid, tr(l, "Pixelgitter (zoom ≥ 8x)", "Pixel grid (zoom ≥ 8x)"));
                });
                ui.menu_button(tr(l, "Hjælp", "Help"), |ui| {
                    if ui.add(egui::Button::new(tr(l, "Genveje", "Shortcuts")).shortcut_text("F1")).clicked() {
                        self.dialog = Dialog::Help;
                        ui.close_menu();
                    }
                    ui.menu_button("Sprog / Language", |ui| {
                        for (lg, name) in [(Lang::Da, "Dansk"), (Lang::En, "English")] {
                            if ui.radio(self.lang == lg, name).clicked() {
                                self.lang = lg;
                                save_lang(lg);
                                self.set_title();
                                ui.close_menu();
                            }
                        }
                    });
                });
            });
        });
    }

    fn tool_panel(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        egui::SidePanel::left("tools").resizable(false).exact_width(130.0).show(ctx, |ui| {
            ui.heading(tr(l, "Værktøjer", "Tools"));
            let prev = self.tool;
            for (t, name) in [
                (Tool::Select, tr(l, "Markér", "Select")),
                (Tool::Pencil, tr(l, "Blyant", "Pencil")),
                (Tool::Brush, tr(l, "Pensel", "Brush")),
                (Tool::Eraser, tr(l, "Viskelæder", "Eraser")),
                (Tool::Fill, tr(l, "Fyld", "Fill")),
                (Tool::Picker, tr(l, "Farvevælger", "Color picker")),
                (Tool::Spray, tr(l, "Spraydåse", "Spray can")),
                (Tool::Line, tr(l, "Linje", "Line")),
                (Tool::Rect, tr(l, "Rektangel", "Rectangle")),
                (Tool::Ellipse, "Ellipse"),
            ] {
                ui.selectable_value(&mut self.tool, t, name);
            }
            if self.tool != prev {
                self.commit_sel();
            }
            ui.separator();
            ui.label(tr(l, "Størrelse", "Size"));
            ui.add(egui::Slider::new(&mut self.size, 1..=64));
            if self.tool == Tool::Brush {
                ui.checkbox(&mut self.round, tr(l, "Rund pensel", "Round brush"));
            }
            if matches!(self.tool, Tool::Rect | Tool::Ellipse) {
                ui.separator();
                ui.label(tr(l, "Udfyldning", "Fill style"));
                ui.radio_value(&mut self.fill_mode, FillMode::Outline, tr(l, "Kun kant", "Outline only"));
                ui.radio_value(&mut self.fill_mode, FillMode::Both, tr(l, "Kant + fyld", "Outline + fill"));
                ui.radio_value(&mut self.fill_mode, FillMode::Filled, tr(l, "Kun fyld", "Fill only"));
            }
            ui.separator();
            ui.small(tr(l, "Venstreklik = forgrund\nHøjreklik = baggrund\nShift = lige/kvadrat", "Left click = foreground\nRight click = background\nShift = straight/square"));
        });
    }

    fn bottom_panel(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr(l, "Forgrund", "Foreground"));
                ui.color_edit_button_srgba(&mut self.fg);
                ui.label(tr(l, "Baggrund", "Background"));
                ui.color_edit_button_srgba(&mut self.bg);
                ui.separator();
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                ui.vertical(|ui| {
                    for row in PALETTE.chunks(10) {
                        ui.horizontal(|ui| {
                            for &(r, g, b) in row {
                                let color = Color32::from_rgb(r, g, b);
                                let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                                ui.painter().rect_filled(rect, 0.0, color);
                                ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::DARK_GRAY));
                                if resp.clicked() {
                                    self.fg = color;
                                }
                                if resp.secondary_clicked() {
                                    self.bg = color;
                                }
                            }
                        });
                    }
                });
            });
            ui.horizontal(|ui| {
                let pos = match self.hover {
                    Some((x, y)) => format!("{}, {}", x, y),
                    None => "–".to_string(),
                };
                ui.label(format!(
                    "Pos: {}   {}: {}x{}   Zoom: {}%   {}",
                    pos,
                    tr(l, "Størrelse", "Size"),
                    self.canvas.width(),
                    self.canvas.height(),
                    (self.zoom * 100.0) as i32,
                    self.status
                ));
            });
        });
    }

    fn handle_pointer(&mut self, ctx: &egui::Context, resp: &egui::Response, origin: Pos2) {
        let z = self.zoom;
        let (pp, ps, d1, d2, pos, shift) = ctx.input(|i| {
            (
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.button_pressed(PointerButton::Secondary),
                i.pointer.button_down(PointerButton::Primary),
                i.pointer.button_down(PointerButton::Secondary),
                i.pointer.latest_pos(),
                i.modifiers.shift,
            )
        });
        let p: Option<P> = pos.map(|p| {
            (((p.x - origin.x) / z).floor() as i32, ((p.y - origin.y) / z).floor() as i32)
        });
        self.hover = if resp.hovered() { p } else { None };

        let active = self.drag.is_some() || self.sel_drag.is_some();
        if !active && (pp || ps) && resp.hovered() {
            if let Some(p) = p {
                self.begin(p, ps && !pp, shift);
            }
        }
        if self.drag.is_some() || self.sel_drag.is_some() {
            if let Some(p) = p {
                self.drag_move(p, shift);
            }
            if !d1 && !d2 {
                self.end();
            } else if self.tool == Tool::Spray {
                ctx.request_repaint();
            }
        }
    }

    fn canvas_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_gray(90)).inner_margin(4.0))
            .show(ctx, |ui| {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .drag_to_scroll(false)
                    .show(ui, |ui| {
                        let size = vec2(
                            self.canvas.width() as f32 * self.zoom,
                            self.canvas.height() as f32 * self.zoom,
                        );
                        let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
                        let origin = resp.rect.min;
                        self.handle_pointer(ctx, &resp, origin);
                        self.refresh(ctx);
                        if let Some(tex) = &self.texture {
                            painter.image(
                                tex.id(),
                                resp.rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        painter.rect_stroke(resp.rect, 0.0, Stroke::new(1.0, Color32::BLACK));

                        // pixelgitter
                        if self.show_grid && self.zoom >= 8.0 {
                            let clip = painter.clip_rect();
                            let z = self.zoom;
                            let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 50));
                            let (w, h) = (self.canvas.width() as f32, self.canvas.height() as f32);
                            let ix0 = ((clip.min.x - origin.x) / z).floor().max(0.0) as i32;
                            let ix1 = ((clip.max.x - origin.x) / z).ceil().min(w) as i32;
                            let iy0 = ((clip.min.y - origin.y) / z).floor().max(0.0) as i32;
                            let iy1 = ((clip.max.y - origin.y) / z).ceil().min(h) as i32;
                            for i in ix0..=ix1 {
                                let x = origin.x + i as f32 * z;
                                painter.line_segment([pos2(x, origin.y), pos2(x, origin.y + h * z)], stroke);
                            }
                            for j in iy0..=iy1 {
                                let y = origin.y + j as f32 * z;
                                painter.line_segment([pos2(origin.x, y), pos2(origin.x + w * z, y)], stroke);
                            }
                        }

                        // markering
                        if let Some(s) = &self.sel {
                            let min = pos2(origin.x + s.x as f32 * self.zoom, origin.y + s.y as f32 * self.zoom);
                            let r = Rect::from_min_size(min, vec2(s.w as f32 * self.zoom, s.h as f32 * self.zoom));
                            painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK));
                            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                            painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, Color32::WHITE), 5.0, 5.0));
                        }
                    });
            });
    }

    fn file_dialog(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        let saving = self.dialog == Dialog::SaveAs;
        let mut go = false;
        let mut cancel = false;
        let mut new_dir: Option<PathBuf> = None;
        let mut pick: Option<String> = None;

        let mut entries: Vec<(String, bool)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dlg_dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let is_dir = e.path().is_dir();
                if is_dir || is_image_name(&name) {
                    entries.push((name, is_dir));
                }
            }
        }
        entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));

        egui::Window::new(if saving { tr(l, "Gem som", "Save as") } else { tr(l, "Åbn", "Open") })
            .collapsible(false)
            .default_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(self.dlg_dir.display().to_string());
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    if ui.button("[..]").clicked() {
                        if let Some(p) = self.dlg_dir.parent() {
                            new_dir = Some(p.to_path_buf());
                        }
                    }
                    for (name, is_dir) in &entries {
                        if *is_dir {
                            if ui.button(format!("[{}]", name)).clicked() {
                                new_dir = Some(self.dlg_dir.join(name));
                            }
                        } else if ui.selectable_label(false, name).clicked() {
                            pick = Some(name.clone());
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Fil:", "File:"));
                    let r = ui.text_edit_singleline(&mut self.path_input);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        go = true;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(if saving { tr(l, "Gem", "Save") } else { tr(l, "Åbn", "Open") }).clicked() {
                        go = true;
                    }
                    if ui.button(tr(l, "Annullér", "Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });

        if let Some(d) = new_dir {
            self.dlg_dir = d;
        }
        if let Some(n) = pick {
            self.path_input = n;
        }
        if cancel {
            self.dialog = Dialog::None;
        }
        if go && !self.path_input.trim().is_empty() {
            let input = PathBuf::from(self.path_input.trim());
            let full = if input.is_absolute() { input } else { self.dlg_dir.join(input) };
            let res = if saving { self.save_to(&full) } else { self.open_file(&full) };
            match res {
                Ok(()) => {
                    self.status = format!("{}: {}", if saving { tr(self.lang, "Gemt", "Saved") } else { tr(self.lang, "Åbnet", "Opened") }, full.display());
                    self.dialog = Dialog::None;
                }
                Err(e) => self.status = format!("{}: {}", tr(self.lang, "Fejl", "Error"), e),
            }
        }
    }

    fn help_dialog(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        let sections: Vec<(&'static str, Vec<(&'static str, &'static str)>)> = vec![
            (
                tr(l, "Fil", "File"),
                vec![
                    ("Ctrl+N", tr(l, "Nyt billede", "New image")),
                    ("Ctrl+O", tr(l, "Åbn", "Open")),
                    ("Ctrl+S", tr(l, "Gem", "Save")),
                    ("Ctrl+Shift+S", tr(l, "Gem som", "Save as")),
                ],
            ),
            (
                tr(l, "Rediger", "Edit"),
                vec![
                    ("Ctrl+Z", tr(l, "Fortryd", "Undo")),
                    ("Ctrl+Y / Ctrl+Shift+Z", tr(l, "Gentag", "Redo")),
                    ("Ctrl+X / Ctrl+C / Ctrl+V", tr(l, "Klip / kopiér / sæt ind", "Cut / copy / paste")),
                    ("Ctrl+A", tr(l, "Markér alt", "Select all")),
                    ("Delete", tr(l, "Slet markering", "Delete selection")),
                    ("Enter", tr(l, "Fastlæg flydende markering", "Commit floating selection")),
                ],
            ),
            (
                tr(l, "Vis", "View"),
                vec![
                    ("Ctrl+Scroll", tr(l, "Zoom ind / ud", "Zoom in / out")),
                    ("F1", tr(l, "Vis/skjul denne hjælp", "Show/hide this help")),
                    ("Esc", tr(l, "Luk hjælp", "Close help")),
                ],
            ),
            (
                tr(l, "Tegning", "Drawing"),
                vec![
                    (tr(l, "Venstre museknap", "Left mouse button"), tr(l, "Tegn med forgrundsfarven", "Draw with the foreground color")),
                    (tr(l, "Højre museknap", "Right mouse button"), tr(l, "Tegn med baggrundsfarven", "Draw with the background color")),
                    (tr(l, "Shift (linje)", "Shift (line)"), tr(l, "Lås til 0° / 45° / 90°", "Snap to 0° / 45° / 90°")),
                    (tr(l, "Shift (rektangel/ellipse)", "Shift (rectangle/ellipse)"), tr(l, "Kvadrat / cirkel", "Square / circle")),
                ],
            ),
            (
                tr(l, "Markering", "Selection"),
                vec![
                    (tr(l, "Træk (markér)", "Drag (select tool)"), tr(l, "Opret rektangulær markering", "Create a rectangular selection")),
                    (tr(l, "Træk i markering", "Drag inside selection"), tr(l, "Flyt markeringen", "Move the selection")),
                    (tr(l, "Klik udenfor", "Click outside"), tr(l, "Fastlæg markeringen", "Commit the selection")),
                ],
            ),
        ];
        let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
        egui::Window::new(tr(l, "Genveje", "Shortcuts"))
            .id(egui::Id::new("help_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                for (idx, (title, rows)) in sections.iter().enumerate() {
                    ui.strong(*title);
                    egui::Grid::new(idx).num_columns(2).spacing([24.0, 2.0]).show(ui, |ui| {
                        for (keys, what) in rows.iter() {
                            ui.monospace(*keys);
                            ui.label(*what);
                            ui.end_row();
                        }
                    });
                    ui.add_space(6.0);
                }
                if ui.button(tr(l, "Luk", "Close")).clicked() {
                    close = true;
                }
            });
        if close {
            self.dialog = Dialog::None;
        }
    }

    fn size_dialog(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        let d = self.dialog;
        let title = match d {
            Dialog::New => tr(l, "Nyt billede", "New image"),
            Dialog::Resize => tr(l, "Ændr lærredets størrelse", "Resize canvas"),
            _ => tr(l, "Skalér billede", "Scale image"),
        };
        let mut ok = false;
        let mut cancel = false;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Bredde:", "Width:"));
                    ui.add(egui::DragValue::new(&mut self.dlg_w).range(1..=MAX_DIM));
                    ui.label(tr(l, "Højde:", "Height:"));
                    ui.add(egui::DragValue::new(&mut self.dlg_h).range(1..=MAX_DIM));
                });
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        ok = true;
                    }
                    if ui.button(tr(l, "Annullér", "Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if ok {
            let (w, h) = (self.dlg_w, self.dlg_h);
            match d {
                Dialog::New => self.new_image(w, h),
                Dialog::Resize => self.resize_canvas(w, h),
                _ => self.stretch(w, h),
            }
            self.dialog = Dialog::None;
        } else if cancel {
            self.dialog = Dialog::None;
        }
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if matches!(self.dialog, Dialog::None | Dialog::Help)
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1))
        {
            self.dialog = if self.dialog == Dialog::Help { Dialog::None } else { Dialog::Help };
        }
        if self.dialog == Dialog::None {
            self.shortcuts(ctx);
        }
        self.menu_bar(ctx);
        self.tool_panel(ctx);
        self.bottom_panel(ctx);
        self.canvas_panel(ctx);
        match self.dialog {
            Dialog::None => {}
            Dialog::Open | Dialog::SaveAs => self.file_dialog(ctx),
            Dialog::Help => self.help_dialog(ctx),
            Dialog::New | Dialog::Resize | Dialog::Stretch => self.size_dialog(ctx),
        }
        if let Some(t) = self.pending_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t));
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_title("Paint"),
        ..Default::default()
    };
    eframe::run_native("rpaint", options, Box::new(|_cc| Ok(Box::new(PaintApp::new()))))
}

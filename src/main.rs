#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use rodio::{OutputStream, OutputStreamHandle, Sink, buffer::SamplesBuffer};
use std::f32::consts::TAU;
use std::time::{Duration, Instant};

const SAMPLE_RATE: u32 = 44_100;

#[derive(Clone, Copy)]
enum Sfx {
    Reveal,
    Flag,
    Explosion,
    Win,
}

fn tone(freq: f32, secs: f32, vol: f32) -> Vec<f32> {
    let n = (SAMPLE_RATE as f32 * secs) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let env = (1.0 - i as f32 / n as f32).powi(2);
            let attack = (i as f32 / 200.0).min(1.0);
            (t * freq * TAU).sin() * vol * env * attack
        })
        .collect()
}

fn explosion() -> Vec<f32> {
    let n = (SAMPLE_RATE as f32 * 0.9) as usize;
    let mut seed: u32 = 0x1234_5678;
    let mut lp: f32 = 0.0f32;
    (0..n)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / 8_388_608.0 - 1.0;
            lp += 0.15 * (noise - lp);
            let p = i as f32 / n as f32;
            let env = (1.0 - p).powi(3);
            let t = i as f32 / SAMPLE_RATE as f32;
            let boom = (t * 55.0 * TAU * (1.0 - p * 0.5)).sin();
            (lp * 1.8 + boom * 0.8) * env * 0.6
        })
        .collect()
}

fn samples_for(sfx: Sfx) -> Vec<f32> {
    match sfx {
        Sfx::Reveal => tone(900.0, 0.03, 0.25),
        Sfx::Flag => {
            let mut v: Vec<f32> = tone(500.0, 0.05, 0.3);
            v.extend(tone(700.0, 0.06, 0.3));
            v
        }
        Sfx::Explosion => explosion(),
        Sfx::Win => {
            let mut v: Vec<f32> = Vec::new();
            for (f, d) in [
                (523.25, 0.12),
                (659.25, 0.12),
                (783.99, 0.12),
                (1046.5, 0.4),
            ] {
                v.extend(tone(f, d, 0.3));
            }
            v
        }
    }
}

struct Sounds {
    stream: Option<(OutputStream, OutputStreamHandle)>,
    enabled: bool,
}

impl Sounds {
    fn new() -> Self {
        Self {
            stream: OutputStream::try_default().ok(),
            enabled: true,
        }
    }

    fn play(&self, sfx: Sfx) {
        if !self.enabled {
            return;
        }
        let Some((_, handle)) = &self.stream else {
            return;
        };
        if let Ok(sink) = Sink::try_new(handle) {
            sink.append(SamplesBuffer::new(1, SAMPLE_RATE, samples_for(sfx)));
            sink.detach();
        }
    }
}

const MINE: u8 = 1;
const REVEALED: u8 = 2;
const FLAGGED: u8 = 4;
const EXPLODED: u8 = 8;

const CELL_HIDDEN: u8 = 9;
const CELL_FLAG: u8 = 10;
const CELL_MINE: u8 = 11;
const CELL_EXPLODED: u8 = 12;
const CELL_WRONG_FLAG: u8 = 13;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GameStatus {
    Ready,
    Playing,
    Won,
    Lost,
}

#[derive(Clone, Copy)]
struct Level {
    name: &'static str,
    width: usize,
    height: usize,
    mines: usize,
}

const LEVELS: [Level; 3] = [
    Level {
        name: "Новачок 9×9, 10 мін",
        width: 9,
        height: 9,
        mines: 10,
    },
    Level {
        name: "Аматор 16×16, 40 мін",
        width: 16,
        height: 16,
        mines: 40,
    },
    Level {
        name: "Профі 30×16, 99 мін",
        width: 30,
        height: 16,
        mines: 99,
    },
];

struct Rng(u64);

impl Rng {
    fn new(seed: u32) -> Self {
        let mut z: u64 = (seed as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self(if z == 0 { 0x2545_F491_4F6C_DD1D } else { z })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x: u64 = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

struct Minesweeper {
    width: usize,
    height: usize,
    mines: usize,
    cells: Vec<u8>,
    adjacent: Vec<u8>,
    status: GameStatus,
    revealed_count: usize,
    flag_count: usize,
    rng: Rng,
}

impl Minesweeper {
    fn new(level: Level) -> Self {
        let total = level.width * level.height;
        Self {
            width: level.width,
            height: level.height,
            mines: level.mines.min(total.saturating_sub(1)),
            cells: vec![0; total],
            adjacent: vec![0; total],
            status: GameStatus::Ready,
            revealed_count: 0,
            flag_count: 0,
            rng: Rng::new(rand_seed()),
        }
    }

    fn reset(&mut self, level: Level) {
        *self = Self::new(level);
    }

    fn active(&self) -> bool {
        matches!(self.status, GameStatus::Ready | GameStatus::Playing)
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.width && y < self.height).then_some(y * self.width + x)
    }

    fn neighbors(&self, index: usize) -> Vec<usize> {
        let x: isize = (index % self.width) as isize;
        let y: isize = (index / self.width) as isize;
        let mut result: Vec<usize> = Vec::with_capacity(8);
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && nx < self.width as isize && ny >= 0 && ny < self.height as isize {
                    result.push(ny as usize * self.width + nx as usize);
                }
            }
        }
        result
    }

    fn place_mines(&mut self, first: usize) {
        let mut protected: Vec<bool> = vec![false; self.cells.len()];
        protected[first] = true;
        for n in self.neighbors(first) {
            protected[n] = true;
        }

        let mut candidates: Vec<usize> = (0..self.cells.len()).filter(|&i| !protected[i]).collect();
        if candidates.len() < self.mines {
            candidates = (0..self.cells.len()).filter(|&i| i != first).collect();
        }

        for k in 0..self.mines {
            let j: usize = k + self.rng.below(candidates.len() - k);
            candidates.swap(k, j);
            self.cells[candidates[k]] |= MINE;
        }

        for i in 0..self.cells.len() {
            if self.cells[i] & MINE == 0 {
                self.adjacent[i] = self
                    .neighbors(i)
                    .iter()
                    .filter(|&&n| self.cells[n] & MINE != 0)
                    .count() as u8;
            }
        }
    }

    fn reveal(&mut self, x: usize, y: usize) -> usize {
        if !self.active() {
            return 0;
        }
        let Some(index) = self.index(x, y) else {
            return 0;
        };
        if self.cells[index] & (REVEALED | FLAGGED) != 0 {
            return 0;
        }
        if self.status == GameStatus::Ready {
            self.place_mines(index);
            self.status = GameStatus::Playing;
        }
        let opened = self.reveal_cell(index);
        self.check_win();
        opened
    }

    fn reveal_cell(&mut self, index: usize) -> usize {
        if self.cells[index] & MINE != 0 {
            self.cells[index] |= EXPLODED;
            self.status = GameStatus::Lost;
            return 0;
        }
        let mut opened: usize = 0;
        let mut stack: Vec<usize> = vec![index];
        while let Some(i) = stack.pop() {
            if self.cells[i] & (REVEALED | FLAGGED) != 0 {
                continue;
            }
            self.cells[i] |= REVEALED;
            self.revealed_count += 1;
            opened += 1;
            if self.adjacent[i] == 0 {
                for n in self.neighbors(i) {
                    if self.cells[n] & (REVEALED | FLAGGED) == 0 {
                        stack.push(n);
                    }
                }
            }
        }
        opened
    }

    fn toggle_flag(&mut self, x: usize, y: usize) -> bool {
        if !self.active() {
            return false;
        }
        let Some(i) = self.index(x, y) else {
            return false;
        };
        if self.cells[i] & REVEALED != 0 {
            return false;
        }
        self.cells[i] ^= FLAGGED;
        if self.cells[i] & FLAGGED != 0 {
            self.flag_count += 1;
        } else {
            self.flag_count = self.flag_count.saturating_sub(1);
        }
        true
    }

    fn chord(&mut self, x: usize, y: usize) -> usize {
        if self.status != GameStatus::Playing {
            return 0;
        }
        let Some(i) = self.index(x, y) else {
            return 0;
        };
        if self.cells[i] & REVEALED == 0 || self.adjacent[i] == 0 {
            return 0;
        }
        let neighbors: Vec<usize> = self.neighbors(i);
        let flagged: usize = neighbors
            .iter()
            .filter(|&&n| self.cells[n] & FLAGGED != 0)
            .count();
        if flagged != self.adjacent[i] as usize {
            return 0;
        }

        let mut opened: usize = 0;
        for n in neighbors {
            if self.cells[n] & (REVEALED | FLAGGED) == 0 {
                opened += self.reveal_cell(n);
                if self.status == GameStatus::Lost {
                    return opened;
                }
            }
        }
        self.check_win();
        opened
    }

    fn check_win(&mut self) {
        if self.status == GameStatus::Playing
            && self.revealed_count == self.cells.len() - self.mines
        {
            self.status = GameStatus::Won;
            for cell in &mut self.cells {
                if *cell & MINE != 0 && *cell & FLAGGED == 0 {
                    *cell |= FLAGGED;
                }
            }
            self.flag_count = self.mines;
        }
    }

    fn view(&self, i: usize) -> u8 {
        let cell: u8 = self.cells[i];
        if self.status == GameStatus::Lost {
            if cell & EXPLODED != 0 {
                return CELL_EXPLODED;
            }
            if cell & MINE != 0 && cell & FLAGGED == 0 {
                return CELL_MINE;
            }
            if cell & FLAGGED != 0 && cell & MINE == 0 {
                return CELL_WRONG_FLAG;
            }
        }
        if cell & REVEALED != 0 {
            self.adjacent[i]
        } else if cell & FLAGGED != 0 {
            CELL_FLAG
        } else {
            CELL_HIDDEN
        }
    }

    fn mines_left(&self) -> isize {
        self.mines as isize - self.flag_count as isize
    }
}

fn rand_seed() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d: Duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    d.subsec_nanos() ^ (d.as_secs() as u32)
}

const CELL_GAP: f32 = 1.0;

const WINDOW_EXTRA: egui::Vec2 = egui::vec2(24.0, 120.0);

fn cell_size_for(width: usize) -> f32 {
    if width >= 30 {
        26.0
    } else if width >= 16 {
        30.0
    } else {
        38.0
    }
}

fn window_size_for(level: Level) -> egui::Vec2 {
    let step: f32 = cell_size_for(level.width) + CELL_GAP;
    egui::vec2(
        level.width as f32 * step + CELL_GAP,
        level.height as f32 * step + CELL_GAP,
    ) + WINDOW_EXTRA
}

struct App {
    game: Minesweeper,
    level_index: usize,
    started: Option<Instant>,
    elapsed: Duration,
    best: [Option<Duration>; 3],
    message: String,
    resize_window: bool,
    field_pressed: bool,
    sounds: Sounds,
}

impl Default for App {
    fn default() -> Self {
        Self {
            game: Minesweeper::new(LEVELS[0]),
            level_index: 0,
            started: None,
            elapsed: Duration::ZERO,
            best: [None; 3],
            message: String::new(),
            resize_window: true,
            field_pressed: false,
            sounds: Sounds::new(),
        }
    }
}

impl App {
    fn new_game(&mut self) {
        self.game.reset(LEVELS[self.level_index]);
        self.started = None;
        self.elapsed = Duration::ZERO;
        self.message.clear();
    }

    fn act(&mut self, x: usize, y: usize, chord: bool) {
        let before: GameStatus = self.game.status;
        let opened: usize = if chord {
            self.game.chord(x, y)
        } else {
            self.game.reveal(x, y)
        };
        if self.game.status == GameStatus::Playing && self.started.is_none() {
            self.started = Some(Instant::now());
        }
        if let Some(start) = self.started {
            if matches!(self.game.status, GameStatus::Won | GameStatus::Lost) {
                self.elapsed = start.elapsed();
                self.started = None;
            }
        }
        if self.game.status == GameStatus::Won {
            let best: &mut Option<Duration> = &mut self.best[self.level_index];
            if best.is_none_or(|b| self.elapsed < b) {
                *best = Some(self.elapsed);
            }
            self.message = "Перемога!".to_owned();
        } else if self.game.status == GameStatus::Lost {
            self.message = "Міна! Гра закінчена".to_owned();
        } else if opened > 0 {
            self.message.clear();
        }

        match self.game.status {
            GameStatus::Won if before != GameStatus::Won => self.sounds.play(Sfx::Win),
            GameStatus::Lost if before != GameStatus::Lost => self.sounds.play(Sfx::Explosion),
            GameStatus::Playing if opened > 0 => self.sounds.play(Sfx::Reveal),
            _ => {}
        }
    }

    fn flag(&mut self, x: usize, y: usize) {
        if self.game.toggle_flag(x, y) {
            self.sounds.play(Sfx::Flag);
        }
    }

    fn current_elapsed(&self) -> Duration {
        match self.started {
            Some(start) => start.elapsed(),
            None => self.elapsed,
        }
    }
}

fn number_color(n: u8) -> egui::Color32 {
    match n {
        1 => egui::Color32::from_rgb(0, 0, 255),
        2 => egui::Color32::from_rgb(0, 128, 0),
        3 => egui::Color32::from_rgb(255, 0, 0),
        4 => egui::Color32::from_rgb(0, 0, 128),
        5 => egui::Color32::from_rgb(128, 0, 0),
        6 => egui::Color32::from_rgb(0, 128, 128),
        7 => egui::Color32::BLACK,
        _ => egui::Color32::from_gray(90),
    }
}

fn draw_flag(painter: &egui::Painter, rect: egui::Rect) {
    let s: f32 = rect.width();
    let c: egui::Pos2 = rect.center();
    let top: f32 = c.y - s * 0.28;
    let bottom: f32 = c.y + s * 0.25;
    let pole_x: f32 = c.x - s * 0.02;
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(pole_x, top),
            egui::pos2(pole_x + s * 0.28, top + s * 0.14),
            egui::pos2(pole_x, top + s * 0.28),
        ],
        egui::Color32::from_rgb(220, 30, 30),
        egui::Stroke::NONE,
    ));
    let black: egui::Stroke = egui::Stroke::new((s * 0.07).max(1.5), egui::Color32::BLACK);
    painter.line_segment([egui::pos2(pole_x, top), egui::pos2(pole_x, bottom)], black);
    painter.line_segment(
        [
            egui::pos2(c.x - s * 0.2, bottom),
            egui::pos2(c.x + s * 0.2, bottom),
        ],
        black,
    );
}

fn draw_mine(painter: &egui::Painter, rect: egui::Rect) {
    let s: f32 = rect.width();
    let c: egui::Pos2 = rect.center();
    let r: f32 = s * 0.22;
    let stroke: egui::Stroke = egui::Stroke::new((s * 0.07).max(1.5), egui::Color32::BLACK);
    let spike: f32 = r * 1.6;
    let d: f32 = spike * std::f32::consts::FRAC_1_SQRT_2;
    for (dx, dy) in [(spike, 0.0), (0.0, spike), (d, d), (d, -d)] {
        painter.line_segment(
            [
                egui::pos2(c.x - dx, c.y - dy),
                egui::pos2(c.x + dx, c.y + dy),
            ],
            stroke,
        );
    }
    painter.circle_filled(c, r, egui::Color32::BLACK);
    painter.circle_filled(
        egui::pos2(c.x - r * 0.35, c.y - r * 0.35),
        r * 0.3,
        egui::Color32::WHITE,
    );
}

fn draw_cross(painter: &egui::Painter, rect: egui::Rect) {
    let s: f32 = rect.width();
    let c: egui::Pos2 = rect.center();
    let k: f32 = s * 0.3;
    let stroke: egui::Stroke =
        egui::Stroke::new((s * 0.09).max(2.0), egui::Color32::from_rgb(200, 0, 0));
    painter.line_segment(
        [egui::pos2(c.x - k, c.y - k), egui::pos2(c.x + k, c.y + k)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(c.x - k, c.y + k), egui::pos2(c.x + k, c.y - k)],
        stroke,
    );
}

fn draw_cell(painter: &egui::Painter, rect: egui::Rect, value: u8) {
    let fill: egui::Color32 = match value {
        CELL_HIDDEN | CELL_FLAG => egui::Color32::from_gray(190),
        CELL_EXPLODED => egui::Color32::from_rgb(230, 80, 80),
        _ => egui::Color32::from_gray(222),
    };

    painter.rect_filled(rect, 0.0, fill);

    match value {
        1..=8 => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                value.to_string(),
                egui::FontId::monospace(rect.height() * 0.65),
                number_color(value),
            );
        }
        CELL_FLAG => draw_flag(painter, rect),
        CELL_MINE | CELL_EXPLODED => draw_mine(painter, rect),
        CELL_WRONG_FLAG => {
            draw_flag(painter, rect);
            draw_cross(painter, rect);
        }
        _ => {}
    }
}

const HEADER_H: f32 = 52.0;

const SEG_MASKS: [u8; 11] = [
    0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F, 0x40,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Smile,
    Surprised,
    Dead,
    Cool,
}

fn draw_bevel(p: &egui::Painter, r: egui::Rect, raised: bool, w: f32) {
    let white: egui::Color32 = egui::Color32::WHITE;
    let dark: egui::Color32 = egui::Color32::from_gray(110);
    let (tl, br) = if raised { (white, dark) } else { (dark, white) };
    p.rect_filled(
        egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, r.min.y + w)),
        0.0,
        tl,
    );
    p.rect_filled(
        egui::Rect::from_min_max(r.min, egui::pos2(r.min.x + w, r.max.y)),
        0.0,
        tl,
    );
    p.rect_filled(
        egui::Rect::from_min_max(egui::pos2(r.min.x, r.max.y - w), r.max),
        0.0,
        br,
    );
    p.rect_filled(
        egui::Rect::from_min_max(egui::pos2(r.max.x - w, r.min.y), r.max),
        0.0,
        br,
    );
}

fn draw_digit(p: &egui::Painter, r: egui::Rect, d: u8) {
    let mask: u8 = SEG_MASKS[d as usize];
    let th: f32 = (r.width() * 0.22).max(2.0);
    let (l, rt, t, b) = (r.left(), r.right(), r.top(), r.bottom());
    let my: f32 = (t + b) / 2.0;
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
        egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
    };
    let segs: [egui::Rect; 7] = [
        rect(l + th, t, rt - th, t + th),
        rect(rt - th, t + th * 0.5, rt, my),
        rect(rt - th, my, rt, b - th * 0.5),
        rect(l + th, b - th, rt - th, b),
        rect(l, my, l + th, b - th * 0.5),
        rect(l, t + th * 0.5, l + th, my),
        rect(l + th, my - th * 0.5, rt - th, my + th * 0.5),
    ];
    let on: egui::Color32 = egui::Color32::from_rgb(255, 0, 0);
    let off: egui::Color32 = egui::Color32::from_rgb(45, 0, 0);
    for (i, s) in segs.iter().enumerate() {
        p.rect_filled(*s, 0.0, if mask & (1 << i) != 0 { on } else { off });
    }
}

fn draw_counter(p: &egui::Painter, rect: egui::Rect, value: i32) {
    p.rect_filled(rect, 0.0, egui::Color32::BLACK);
    draw_bevel(p, rect, false, 2.0);
    let digits: [u8; 3] = if value < 0 {
        let a = (-value).min(99);
        [10, (a / 10) as u8, (a % 10) as u8]
    } else {
        let a = value.min(999);
        [(a / 100) as u8, ((a / 10) % 10) as u8, (a % 10) as u8]
    };
    let inner: egui::Rect = rect.shrink2(egui::vec2(4.0, 4.0));
    let dw: f32 = inner.width() / 3.0;
    for (i, d) in digits.iter().enumerate() {
        let dr = egui::Rect::from_min_size(
            inner.min + egui::vec2(i as f32 * dw, 0.0),
            egui::vec2(dw, inner.height()),
        )
        .shrink2(egui::vec2(2.0, 0.0));
        draw_digit(p, dr, *d);
    }
}

fn draw_face(p: &egui::Painter, rect: egui::Rect, face: Face, pressed: bool) {
    p.rect_filled(rect, 0.0, egui::Color32::from_gray(192));
    if pressed {
        p.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::from_gray(110)),
            egui::StrokeKind::Inside,
        );
    } else {
        draw_bevel(p, rect, true, 2.0);
    }
    let off: egui::Vec2 = if pressed {
        egui::vec2(1.0, 1.0)
    } else {
        egui::Vec2::ZERO
    };
    let c: egui::Pos2 = rect.center() + off;
    let r: f32 = rect.width() * 0.36;
    let black: egui::Color32 = egui::Color32::BLACK;
    let line: egui::Stroke = egui::Stroke::new((r * 0.1).max(1.0), black);

    p.circle(
        c,
        r,
        egui::Color32::from_rgb(255, 255, 0),
        egui::Stroke::new(1.0, black),
    );

    let eye_l: egui::Pos2 = egui::pos2(c.x - r * 0.38, c.y - r * 0.22);
    let eye_r: egui::Pos2 = egui::pos2(c.x + r * 0.38, c.y - r * 0.22);

    let curve = |smile: bool| {
        let mut pts: Vec<egui::Pos2> = Vec::new();
        for k in -6..=6 {
            let t: f32 = k as f32 / 6.0;
            let bow: f32 = r * 0.3 * (1.0 - t * t);
            let y: f32 = if smile {
                c.y + r * 0.25 + bow
            } else {
                c.y + r * 0.55 - bow
            };
            pts.push(egui::pos2(c.x + t * r * 0.5, y));
        }
        egui::Shape::line(pts, line)
    };

    match face {
        Face::Smile => {
            p.circle_filled(eye_l, r * 0.12, black);
            p.circle_filled(eye_r, r * 0.12, black);
            p.add(curve(true));
        }
        Face::Surprised => {
            p.circle_filled(eye_l, r * 0.12, black);
            p.circle_filled(eye_r, r * 0.12, black);
            p.circle_filled(egui::pos2(c.x, c.y + r * 0.42), r * 0.2, black);
        }
        Face::Dead => {
            let k = r * 0.16;
            for e in [eye_l, eye_r] {
                p.line_segment([e + egui::vec2(-k, -k), e + egui::vec2(k, k)], line);
                p.line_segment([e + egui::vec2(-k, k), e + egui::vec2(k, -k)], line);
            }
            p.add(curve(false));
        }
        Face::Cool => {
            let y0: f32 = c.y - r * 0.42;
            let y1: f32 = c.y - r * 0.02;
            p.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - r * 0.75, y0),
                    egui::pos2(c.x - r * 0.08, y1),
                ),
                2.0,
                black,
            );
            p.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(c.x + r * 0.08, y0),
                    egui::pos2(c.x + r * 0.75, y1),
                ),
                2.0,
                black,
            );
            p.line_segment(
                [
                    egui::pos2(c.x - r * 0.1, y0 + r * 0.1),
                    egui::pos2(c.x + r * 0.1, y0 + r * 0.1),
                ],
                line,
            );
            p.add(curve(true));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.started.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if ui.input(|i| i.key_pressed(egui::Key::F2)) {
            self.new_game();
        }
        if self.resize_window {
            self.resize_window = false;
            let size = window_size_for(LEVELS[self.level_index]);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }

        egui::CentralPanel::default().show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Гра", |ui| {
                    if ui.button("Нова гра  (F2)").clicked() {
                        self.new_game();
                        ui.close();
                    }
                    ui.separator();
                    for (i, level) in LEVELS.iter().enumerate() {
                        if ui
                            .radio_value(&mut self.level_index, i, level.name)
                            .clicked()
                        {
                            self.new_game();
                            self.resize_window = true;
                            ui.close();
                        }
                    }
                    ui.separator();
                    match self.best[self.level_index] {
                        Some(b) => ui.label(format!("Рекорд: {:.1} с", b.as_secs_f64())),
                        None => ui.label("Рекорд: —"),
                    };
                    ui.separator();
                    ui.checkbox(&mut self.sounds.enabled, "Звук");
                    ui.separator();
                    if ui.button("Вихід").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Довідка", |ui| {
                    ui.label("ЛКМ по закритій клітині - відкрити");
                    ui.label("ПКМ - поставити/зняти прапор");
                    ui.label("ЛКМ за цифрою - відкрити сусідів, якщо поруч стільки ж прапорів");
                    ui.label("Перший хід завжди безпечний");
                });
            });

            ui.separator();

            ui.add_space(2.0);

            let cell_size: f32 = cell_size_for(self.game.width);
            let gap: f32 = CELL_GAP;
            let step: f32 = cell_size + gap;
            let (w, h) = (self.game.width, self.game.height);
            let total: egui::Vec2 = egui::vec2(w as f32 * step + gap, h as f32 * step + gap);

            egui::ScrollArea::both().show(ui, |ui| {
                let (hrect, _) =
                    ui.allocate_exact_size(egui::vec2(total.x, HEADER_H), egui::Sense::hover());

                let hp = ui.painter_at(hrect);

                ui.separator();

                let counter_size: egui::Vec2 = egui::vec2(66.0, 36.0);
                let left: egui::Rect = egui::Rect::from_min_size(
                    egui::pos2(hrect.left() + 8.0, hrect.center().y - counter_size.y / 2.0),
                    counter_size,
                );
                let right: egui::Rect = egui::Rect::from_min_size(
                    egui::pos2(hrect.right() - 8.0 - counter_size.x, left.top()),
                    counter_size,
                );

                draw_counter(&hp, left, self.game.mines_left() as i32);
                draw_counter(&hp, right, self.current_elapsed().as_secs().min(999) as i32);

                let face_rect: egui::Rect =
                    egui::Rect::from_center_size(hrect.center(), egui::vec2(38.0, 38.0));
                let face_resp: egui::Response = ui.interact(
                    face_rect,
                    ui.make_persistent_id("face_button"),
                    egui::Sense::click(),
                );
                let face = match self.game.status {
                    GameStatus::Lost => Face::Dead,
                    GameStatus::Won => Face::Cool,
                    _ if self.field_pressed => Face::Surprised,
                    _ => Face::Smile,
                };

                draw_face(&hp, face_rect, face, face_resp.is_pointer_button_down_on());

                if face_resp.clicked() {
                    self.new_game();
                }

                ui.add_space(2.0);

                let (rect, response) = ui.allocate_exact_size(total, egui::Sense::click());
                self.field_pressed = response.is_pointer_button_down_on() && self.game.active();
                let painter: egui::Painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(120));

                let origin: egui::Pos2 = rect.min + egui::vec2(gap, gap);
                for y in 0..h {
                    for x in 0..w {
                        let min: egui::Pos2 = origin + egui::vec2(x as f32 * step, y as f32 * step);
                        let cell_rect =
                            egui::Rect::from_min_size(min, egui::vec2(cell_size, cell_size));
                        draw_cell(&painter, cell_rect, self.game.view(y * w + x));
                    }
                }

                let clicked: bool = response.clicked();
                let secondary: bool = response.secondary_clicked();
                if clicked || secondary {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let rel: egui::Vec2 = pos - origin;
                        if rel.x >= 0.0 && rel.y >= 0.0 {
                            let x: usize = (rel.x / step) as usize;
                            let y: usize = (rel.y / step) as usize;
                            if x < w && y < h {
                                if secondary {
                                    self.flag(x, y);
                                } else {
                                    let value: u8 = self.game.view(y * w + x);
                                    if (1..=8).contains(&value) {
                                        self.act(x, y, true);
                                    } else {
                                        self.act(x, y, false);
                                    }
                                }
                            }
                        }
                    }
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let icon: egui::IconData =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
            .expect("failed to read the icon");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_size_for(LEVELS[0]))
            .with_resizable(false)
            .with_maximize_button(false)
            .with_title("Сапер")
            .with_icon(icon),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native("Сапер", options, Box::new(|_cc| Ok(Box::<App>::default())))
}

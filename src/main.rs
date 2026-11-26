use eframe::egui;
use std::time::{Duration, Instant};

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
        name: "Новичок 9×9, 10 мин",
        width: 9,
        height: 9,
        mines: 10,
    },
    Level {
        name: "Аматор 16×16, 40 мин",
        width: 16,
        height: 16,
        mines: 40,
    },
    Level {
        name: "Профи 30×16, 99 мин",
        width: 30,
        height: 16,
        mines: 99,
    },
];

struct Rng(u64);

impl Rng {
    fn new(seed: u32) -> Self {
        let mut z = (seed as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self(if z == 0 { 0x2545_F491_4F6C_DD1D } else { z })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
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
        let x = (index % self.width) as isize;
        let y = (index / self.width) as isize;
        let mut result = Vec::with_capacity(8);
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
        let mut protected = vec![false; self.cells.len()];
        protected[first] = true;
        for n in self.neighbors(first) {
            protected[n] = true;
        }

        let mut candidates: Vec<usize> = (0..self.cells.len()).filter(|&i| !protected[i]).collect();
        if candidates.len() < self.mines {
            candidates = (0..self.cells.len()).filter(|&i| i != first).collect();
        }

        for k in 0..self.mines {
            let j = k + self.rng.below(candidates.len() - k);
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
        let mut opened = 0;
        let mut stack = vec![index];
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
        let neighbors = self.neighbors(i);
        let flagged = neighbors
            .iter()
            .filter(|&&n| self.cells[n] & FLAGGED != 0)
            .count();
        if flagged != self.adjacent[i] as usize {
            return 0;
        }

        let mut opened = 0;
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
        let cell = self.cells[i];
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
    // Seed from the system clock without an extra dependency.
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        ^ (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32)
}

struct App {
    game: Minesweeper,
    level_index: usize,
    started: Option<Instant>,
    elapsed: Duration,
    best: [Option<Duration>; 3],
    message: String,
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
        let before = self.game.status;
        let opened = if chord {
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
            let best = &mut self.best[self.level_index];
            if best.is_none_or(|b| self.elapsed < b) {
                *best = Some(self.elapsed);
            }
            self.message = "🎉 Победа!".to_owned();
        } else if self.game.status == GameStatus::Lost {
            self.message = "💥 Мина! Игра окончена".to_owned();
        } else if opened > 0 {
            self.message.clear();
        }
        if before == self.game.status && opened == 0 && !chord {
            // No-op, kept intentionally: clicking an already-open cell is handled below.
        }
    }

    fn flag(&mut self, x: usize, y: usize) {
        self.game.toggle_flag(x, y);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.started.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Сапёр");
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("level")
                    .selected_text(LEVELS[self.level_index].name)
                    .show_ui(ui, |ui| {
                        for (i, level) in LEVELS.iter().enumerate() {
                            if ui
                                .selectable_value(&mut self.level_index, i, level.name)
                                .clicked()
                            {
                                self.new_game();
                            }
                        }
                    });

                ui.label(format!("💣 {:03}", self.game.mines_left()));
                if ui.button("🙂 Заново").clicked() {
                    self.new_game();
                }
                ui.label(format!("⏱ {:03}", self.elapsed.as_secs().min(999)));
                if let Some(best) = self.best[self.level_index] {
                    ui.label(format!("Рекорд: {:.1} с", best.as_secs_f64()));
                }
            });

            if !self.message.is_empty() {
                ui.label(&self.message);
            } else if self.game.status == GameStatus::Ready {
                ui.label("Нажмите на клетку, чтобы начать. Первый ход безопасный.");
            }

            let cell_size = if self.game.width >= 30 {
                24.0
            } else if self.game.width >= 16 {
                28.0
            } else {
                36.0
            };
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("minefield")
                    .spacing(egui::vec2(2.0, 2.0))
                    .show(ui, |ui| {
                        for y in 0..self.game.height {
                            for x in 0..self.game.width {
                                let i = y * self.game.width + x;
                                let value = self.game.view(i);
                                let (text, color) = match value {
                                    1 => ("1", egui::Color32::BLUE),
                                    2 => ("2", egui::Color32::GREEN),
                                    3 => ("3", egui::Color32::RED),
                                    4 => ("4", egui::Color32::from_rgb(0, 0, 128)),
                                    5 => ("5", egui::Color32::from_rgb(128, 0, 0)),
                                    6 => ("6", egui::Color32::from_rgb(0, 128, 128)),
                                    7 => ("7", egui::Color32::BLACK),
                                    8 => ("8", egui::Color32::GRAY),
                                    CELL_FLAG => ("🚩", egui::Color32::WHITE),
                                    CELL_MINE | CELL_EXPLODED => ("💣", egui::Color32::WHITE),
                                    CELL_WRONG_FLAG => ("❌", egui::Color32::WHITE),
                                    _ => ("", egui::Color32::BLACK),
                                };
                                let fill = match value {
                                    CELL_HIDDEN | CELL_FLAG => egui::Color32::from_gray(105),
                                    CELL_EXPLODED => egui::Color32::from_rgb(220, 70, 70),
                                    _ => egui::Color32::from_gray(205),
                                };
                                let button = egui::Button::new(
                                    egui::RichText::new(text).color(color).strong(),
                                )
                                .fill(fill)
                                .min_size(egui::vec2(cell_size, cell_size));

                                let response = ui.add(button);
                                if response.clicked() {
                                    if (1..=8).contains(&value) {
                                        self.act(x, y, true);
                                    } else {
                                        self.act(x, y, false);
                                    }
                                }
                                if response.secondary_clicked() {
                                    self.flag(x, y);
                                }
                                response.on_hover_text("ЛКМ — открыть; ПКМ — флаг");
                            }
                            ui.end_row();
                        }
                    });
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([850.0, 650.0])
            .with_min_inner_size([360.0, 300.0])
            .with_title("Сапёр"),

        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native("Сапёр", options, Box::new(|_cc| Ok(Box::<App>::default())))
}

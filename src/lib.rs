use wasm_bindgen::prelude::*;

const MINE: u8 = 1;
const REVEALED: u8 = 2;
const FLAGGED: u8 = 4;
const EXPLODED: u8 = 8;

pub const CELL_HIDDEN: u8 = 9;
pub const CELL_FLAG: u8 = 10;
pub const CELL_MINE: u8 = 11;
pub const CELL_EXPLODED: u8 = 12;
pub const CELL_WRONG_FLAG: u8 = 13;

#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameStatus {
    Ready = 0,
    Playing = 1,
    Won = 2,
    Lost = 3,
}

struct Rng(u64);

impl Rng {
    fn new(seed: u32) -> Self {
        let mut z = (seed as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Rng(if z == 0 { 0x2545_F491_4F6C_DD1D } else { z })
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

#[wasm_bindgen]
pub struct Minesweeper {
    width: usize,
    height: usize,
    mines: usize,
    flags: Vec<u8>,
    adjacent: Vec<u8>,
    status: GameStatus,
    revealed_count: usize,
    flag_count: usize,
    seed: u32,
    rng: Rng,
}

#[wasm_bindgen]
impl Minesweeper {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, mines: u32, seed: u32) -> Minesweeper {
        let width = width.clamp(1, 1000) as usize;
        let height = height.clamp(1, 1000) as usize;
        let total = width * height;

        let mines = (mines as usize).min(total.saturating_sub(1));

        Minesweeper {
            width,
            height,
            mines,
            flags: vec![0; total],
            adjacent: vec![0; total],
            status: GameStatus::Ready,
            revealed_count: 0,
            flag_count: 0,
            seed,
            rng: Rng::new(seed),
        }
    }

    pub fn reset(&mut self, seed: u32) {
        self.seed = seed;
        self.rng = Rng::new(seed);
        self.flags.iter_mut().for_each(|c| *c = 0);
        self.adjacent.iter_mut().for_each(|c| *c = 0);
        self.status = GameStatus::Ready;
        self.revealed_count = 0;
        self.flag_count = 0;
    }

    pub fn width(&self) -> u32 {
        self.width as u32
    }

    pub fn height(&self) -> u32 {
        self.height as u32
    }

    pub fn mines(&self) -> u32 {
        self.mines as u32
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    pub fn status(&self) -> GameStatus {
        self.status
    }

    pub fn mines_left(&self) -> i32 {
        self.mines as i32 - self.flag_count as i32
    }

    pub fn flags_placed(&self) -> u32 {
        self.flag_count as u32
    }

    pub fn reveal(&mut self, x: u32, y: u32) -> u32 {
        if !self.is_active() {
            return 0;
        }
        let Some(idx) = self.index(x, y) else {
            return 0;
        };
        if self.flags[idx] & (REVEALED | FLAGGED) != 0 {
            return 0;
        }

        if self.status == GameStatus::Ready {
            self.place_mines(idx);
            self.status = GameStatus::Playing;
        }

        let opened = self.reveal_cell(idx);
        self.check_win();
        opened
    }

    pub fn toggle_flag(&mut self, x: u32, y: u32) -> bool {
        if !self.is_active() {
            return false;
        }
        let Some(idx) = self.index(x, y) else {
            return false;
        };
        if self.flags[idx] & REVEALED != 0 {
            return false;
        }
        self.flags[idx] ^= FLAGGED;
        if self.flags[idx] & FLAGGED != 0 {
            self.flag_count += 1;
        } else {
            self.flag_count -= 1;
        }
        true
    }

    pub fn chord(&mut self, x: u32, y: u32) -> u32 {
        if self.status != GameStatus::Playing {
            return 0;
        }
        let Some(idx) = self.index(x, y) else {
            return 0;
        };
        if self.flags[idx] & REVEALED == 0 || self.adjacent[idx] == 0 {
            return 0;
        }

        let (neighbors, n) = self.neighbors(idx);
        let neighbors = &neighbors[..n];

        let flagged = neighbors
            .iter()
            .filter(|&&i| self.flags[i] & FLAGGED != 0)
            .count();
        if flagged != self.adjacent[idx] as usize {
            return 0;
        }

        let mut opened = 0;
        for &i in neighbors {
            if self.flags[i] & (REVEALED | FLAGGED) == 0 {
                opened += self.reveal_cell(i);
                if self.status == GameStatus::Lost {
                    return opened;
                }
            }
        }
        self.check_win();
        opened
    }

    pub fn view(&self) -> Vec<u8> {
        let finished_lost = self.status == GameStatus::Lost;
        self.flags
            .iter()
            .zip(self.adjacent.iter())
            .map(|(&f, &adj)| {
                if finished_lost {
                    if f & EXPLODED != 0 {
                        return CELL_EXPLODED;
                    }
                    if f & MINE != 0 && f & FLAGGED == 0 {
                        return CELL_MINE;
                    }
                    if f & FLAGGED != 0 && f & MINE == 0 {
                        return CELL_WRONG_FLAG;
                    }
                }
                if f & REVEALED != 0 {
                    adj
                } else if f & FLAGGED != 0 {
                    CELL_FLAG
                } else {
                    CELL_HIDDEN
                }
            })
            .collect()
    }

    pub fn cell(&self, x: u32, y: u32) -> u8 {
        match self.index(x, y) {
            Some(idx) => self.view()[idx],
            None => 255,
        }
    }
}

impl Minesweeper {
    fn is_active(&self) -> bool {
        matches!(self.status, GameStatus::Ready | GameStatus::Playing)
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        let (x, y) = (x as usize, y as usize);
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }

    fn neighbors(&self, idx: usize) -> ([usize; 8], usize) {
        let x = (idx % self.width) as isize;
        let y = (idx / self.width) as isize;
        let (w, h) = (self.width as isize, self.height as isize);
        let mut out = [0usize; 8];
        let mut n = 0;
        for dy in -1..=1isize {
            for dx in -1..=1isize {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && nx < w && ny >= 0 && ny < h {
                    out[n] = (ny * w + nx) as usize;
                    n += 1;
                }
            }
        }
        (out, n)
    }

    fn place_mines(&mut self, first: usize) {
        let total = self.width * self.height;

        let (nb, n) = self.neighbors(first);
        let mut protected = vec![false; total];
        protected[first] = true;
        for &i in &nb[..n] {
            protected[i] = true;
        }

        let mut candidates: Vec<usize> = (0..total).filter(|&i| !protected[i]).collect();
        if candidates.len() < self.mines {
            candidates = (0..total).filter(|&i| i != first).collect();
        }

        for k in 0..self.mines {
            let j = k + self.rng.below(candidates.len() - k);
            candidates.swap(k, j);
            self.flags[candidates[k]] |= MINE;
        }

        for idx in 0..total {
            if self.flags[idx] & MINE != 0 {
                continue;
            }
            let (nb, n) = self.neighbors(idx);
            self.adjacent[idx] = nb[..n]
                .iter()
                .filter(|&&i| self.flags[i] & MINE != 0)
                .count() as u8;
        }
    }

    fn reveal_cell(&mut self, idx: usize) -> u32 {
        if self.flags[idx] & MINE != 0 {
            self.flags[idx] |= EXPLODED;
            self.status = GameStatus::Lost;
            return 0;
        }

        let mut opened = 0u32;
        let mut stack = vec![idx];
        while let Some(i) = stack.pop() {
            if self.flags[i] & (REVEALED | FLAGGED) != 0 {
                continue;
            }
            self.flags[i] |= REVEALED;
            self.revealed_count += 1;
            opened += 1;

            if self.adjacent[i] == 0 {
                let (nb, n) = self.neighbors(i);
                for &j in &nb[..n] {
                    if self.flags[j] & (REVEALED | FLAGGED) == 0 {
                        stack.push(j);
                    }
                }
            }
        }
        opened
    }

    fn check_win(&mut self) {
        if self.status == GameStatus::Playing
            && self.revealed_count == self.width * self.height - self.mines
        {
            self.status = GameStatus::Won;

            for f in self.flags.iter_mut() {
                if *f & MINE != 0 && *f & FLAGGED == 0 {
                    *f |= FLAGGED;
                }
            }
            self.flag_count = self.mines;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_mines(width: usize, height: usize, mines: &[(usize, usize)]) -> Minesweeper {
        let mut g = Minesweeper::new(width as u32, height as u32, mines.len() as u32, 0);
        for &(x, y) in mines {
            g.flags[y * width + x] |= MINE;
        }
        for idx in 0..width * height {
            if g.flags[idx] & MINE != 0 {
                continue;
            }
            let (nb, n) = g.neighbors(idx);
            g.adjacent[idx] = nb[..n].iter().filter(|&&i| g.flags[i] & MINE != 0).count() as u8;
        }
        g.status = GameStatus::Playing;
        g
    }

    #[test]
    fn first_click_is_safe() {
        for seed in 0..200 {
            let mut g = Minesweeper::new(9, 9, 10, seed);
            g.reveal(4, 4);
            assert_ne!(g.status(), GameStatus::Lost);
            let mines = g.flags.iter().filter(|&&f| f & MINE != 0).count();
            assert_eq!(mines, 10);
        }
    }

    #[test]
    fn flood_fill_opens_empty_area() {
        let mut g = with_mines(5, 5, &[(4, 4)]);
        let opened = g.reveal(0, 0);
        assert_eq!(opened, 24);
        assert_eq!(g.status(), GameStatus::Won);
    }

    #[test]
    fn stepping_on_mine_loses() {
        let mut g = with_mines(3, 3, &[(1, 1)]);
        g.reveal(1, 1);
        assert_eq!(g.status(), GameStatus::Lost);
        assert_eq!(g.view()[4], CELL_EXPLODED);
    }

    #[test]
    fn flags_block_reveal_and_count() {
        let mut g = with_mines(3, 3, &[(0, 0)]);
        assert!(g.toggle_flag(1, 1));
        assert_eq!(g.mines_left(), 0);
        assert_eq!(g.reveal(1, 1), 0);
        assert!(g.toggle_flag(1, 1));
        assert_eq!(g.mines_left(), 1);
    }

    #[test]
    fn chord_opens_neighbors() {
        let mut g = with_mines(4, 4, &[(0, 0)]);
        g.reveal(1, 1);

        assert_eq!(g.chord(1, 1), 0);

        g.toggle_flag(0, 0);
        assert!(g.chord(1, 1) > 0);
        assert_eq!(g.status(), GameStatus::Won);
    }

    #[test]
    fn same_seed_same_field() {
        let mut a = Minesweeper::new(16, 16, 40, 12345);
        let mut b = Minesweeper::new(16, 16, 40, 12345);
        a.reveal(5, 5);
        b.reveal(5, 5);
        assert_eq!(a.view(), b.view());
    }

    #[test]
    fn chord_with_wrong_flag_loses() {
        let mut g = with_mines(4, 4, &[(0, 0)]);
        g.reveal(1, 1);
        g.toggle_flag(1, 0);
        g.chord(1, 1);
        assert_eq!(g.status(), GameStatus::Lost);
    }
}

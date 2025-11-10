/*
==============================================================
   ████████╗███████╗████████╗██████╗ ██╗███████╗███████╗
   ╚══██╔══╝██╔════╝╚══██╔══╝██╔══██╗██║██╔════╝██╔════╝
      ██║   █████╗     ██║   ██████╔╝██║█████╗  ███████╗
      ██║   ██╔══╝     ██║   ██╔══██╗██║██╔══╝  ╚════██║
      ██║   ███████╗   ██║   ██║  ██║██║███████╗███████║
      ╚═╝   ╚══════╝   ╚═╝   ╚═╝  ╚═╝╚═╝╚══════╝╚══════╝

        TETRIS-LIKE GAME in Rust (Terminal Edition)
        Soda
--------------------------------------------------------------
Controls:
    ← / →   Move piece left or right
    ↓        Soft drop (faster)
    ↑        Rotate (clockwise)
    Space     Hard drop (instantly drop)
    Q or ESC  Quit
--------------------------------------------------------------
Requirements:
  • Runs on any Linux terminal that supports UTF-8
  • Built with: `cargo build --release`
  • Run: `./target/release/tetris_rust`
--------------------------------------------------------------
Developer Notes:
  - Colors added with crossterm::style
  - No microtransactions. 100% ad-free. No DLC.
==============================================================
*/
// [imports]
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    style::{Color, SetBackgroundColor, ResetColor, Print},
    terminal::{self, ClearType},
    QueueableCommand,
};
use rand::seq::SliceRandom;
use rand::thread_rng;
use rodio::{Decoder, OutputStream, Sink, Source};
use std::cmp::max;
use std::io::{stdout, Write};
use std::time::{Duration, Instant};
use std::{thread, io::Cursor};

// [constants]
const WIDTH: usize = 10;
const HEIGHT: usize = 20;
const TICK_MS: u64 = 500;

// [types]
#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Empty,
    Filled(u8),
}

type Board = [[Cell; WIDTH]; HEIGHT];

#[derive(Clone)]
struct Piece {
    blocks: Vec<(i32, i32)>,
    x: i32,
    y: i32,
    id: u8,
}

// [piece implementation]
impl Piece {
    fn shape_rotations(name: usize) -> Vec<Vec<(i32, i32)>> {
        let shapes0 = vec![
            vec![(-2, 0), (-1, 0), (0, 0), (1, 0)],
            vec![(0, 0), (1, 0), (0, 1), (1, 1)],
            vec![(-1, 0), (0, 0), (1, 0), (0, 1)],
            vec![(-1, 0), (0, 0), (1, 0), (1, 1)],
            vec![(-1, 0), (0, 0), (1, 0), (-1, 1)],
            vec![(-1, 1), (0, 1), (0, 0), (1, 0)],
            vec![(-1, 0), (0, 0), (0, 1), (1, 1)],
        ];
        let base = &shapes0[name];
        let mut rots = vec![];
        let mut current = base.clone();
        for _ in 0..4 {
            rots.push(current.clone());
            current = current.iter().map(|(x, y)| (-*y, *x)).collect();
        }
        rots
    }

    fn random_spawn() -> Self {
        let mut rng = thread_rng();
        let idx = (0..7).collect::<Vec<_>>().choose(&mut rng).cloned().unwrap();
        let rots = Piece::shape_rotations(idx);
        let blocks = rots[0].clone();
        let x = (WIDTH / 2) as i32;
        let y = -2;
        Piece {
            blocks,
            x,
            y,
            id: (idx + 1) as u8,
        }
    }

    fn rotate_cw(&mut self) {
        self.blocks = self.blocks.iter().map(|(x, y)| (-*y, *x)).collect();
    }

    fn positions(&self) -> Vec<(i32, i32)> {
        self.blocks.iter().map(|(bx, by)| (self.x + *bx, self.y + *by)).collect()
    }
}

// [game logic]
fn create_empty_board() -> Board {
    [[Cell::Empty; WIDTH]; HEIGHT]
}

fn collides(board: &Board, piece: &Piece) -> bool {
    for (x, y) in piece.positions() {
        if x < 0 || x >= WIDTH as i32 || y >= HEIGHT as i32 {
            return true;
        }
        if y >= 0 {
            if let Cell::Filled(_) = board[y as usize][x as usize] {
                return true;
            }
        }
    }
    false
}

fn lock_piece(board: &mut Board, piece: &Piece) {
    for (x, y) in piece.positions() {
        if y >= 0 && y < HEIGHT as i32 && x >= 0 && x < WIDTH as i32 {
            board[y as usize][x as usize] = Cell::Filled(piece.id);
        }
    }
}

fn clear_lines(board: &mut Board) -> usize {
    let mut write_row = HEIGHT as i32 - 1;
    let mut cleared = 0usize;

    for read_row in (0..HEIGHT as i32).rev() {
        let full = (0..WIDTH).all(|c| matches!(board[read_row as usize][c], Cell::Filled(_)));
        if full {
            cleared += 1;
            continue;
        }
        if write_row != read_row {
            for c in 0..WIDTH {
                board[write_row as usize][c] = board[read_row as usize][c];
            }
        }
        write_row -= 1;
    }

    for r in 0..=write_row {
        for c in 0..WIDTH {
            board[r as usize][c] = Cell::Empty;
        }
    }
    cleared
}

fn piece_color(id: u8) -> Color {
    match id {
        1 => Color::Cyan,
        2 => Color::Yellow,
        3 => Color::Magenta,
        4 => Color::Blue,
        5 => Color::Red,
        6 => Color::Green,
        7 => Color::DarkRed,
        _ => Color::White,
    }
}

// [rendering]
fn draw_board(stdout: &mut impl Write, board: &Board, piece: &Piece, next_piece: &Piece, score: usize) -> crossterm::Result<()> {
    stdout.queue(cursor::Hide)?;
    stdout.queue(terminal::Clear(ClearType::All))?;

    stdout.queue(cursor::MoveTo(0, 0))?;
    writeln!(stdout, "+{}+", "-".repeat(WIDTH * 2))?;

    for r in 0..HEIGHT {
        stdout.queue(cursor::MoveTo(0, (r + 1) as u16))?;
        write!(stdout, "|")?;
        for c in 0..WIDTH {
            match board[r][c] {
                Cell::Empty => {
                    stdout.queue(ResetColor)?;
                    stdout.queue(Print("  "))?;
                }
                Cell::Filled(id) => {
                    let col = piece_color(id);
                    stdout.queue(SetBackgroundColor(col))?;
                    stdout.queue(Print("  "))?;
                    stdout.queue(ResetColor)?;
                }
            }
        }
        writeln!(stdout, "|")?;
    }

    stdout.queue(cursor::MoveTo(0, (HEIGHT + 1) as u16))?;
    writeln!(stdout, "+{}+", "-".repeat(WIDTH * 2))?;

    for (x, y) in piece.positions() {
        if y >= 0 && y < HEIGHT as i32 && x >= 0 && x < WIDTH as i32 {
            let col_pos = 1 + x * 2;
            let row_pos = 1 + y;
            stdout.queue(cursor::MoveTo(col_pos as u16, row_pos as u16))?;
            let col = piece_color(piece.id);
            stdout.queue(SetBackgroundColor(col))?;
            stdout.queue(Print("  "))?;
            stdout.queue(ResetColor)?;
        }
    }

    let info_y = (HEIGHT + 3) as u16;
    stdout.queue(cursor::MoveTo(0, info_y))?;
    writeln!(stdout, "Score: {}", score)?;
    writeln!(stdout, "Controls: ← → ↓ ↑ Space  Q")?;
    writeln!(stdout, "Next:")?;

    for (bx, by) in &next_piece.blocks {
        let col = piece_color(next_piece.id);
        stdout.queue(SetBackgroundColor(col))?;
        stdout.queue(cursor::MoveTo((2 + bx * 2) as u16, info_y + 4 + *by as u16))?;
        stdout.queue(Print("  "))?;
        stdout.queue(ResetColor)?;
    }

    stdout.flush()?;
    Ok(())
}

// [music]
fn play_music() {
    thread::spawn(|| {
        let bytes = include_bytes!("../assets/791018.mp3");
        let cursor = Cursor::new(bytes);
        if let Ok((_stream, handle)) = OutputStream::try_default() {
            if let Ok(source) = Decoder::new(cursor) {
                let sink = Sink::try_new(&handle).unwrap();
                sink.append(source.repeat_infinite());
                sink.sleep_until_end();
            }
        }
    });
}

// [game loop]
fn main() -> crossterm::Result<()> {
    play_music();
    let mut stdout = stdout();
    crossterm::execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
    terminal::enable_raw_mode()?;
    let result = run_game(&mut stdout);
    crossterm::execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}

fn run_game(stdout: &mut impl Write) -> crossterm::Result<()> {
    let mut board = create_empty_board();
    let mut current = Piece::random_spawn();
    let mut next = Piece::random_spawn();
    let mut last_tick = Instant::now();
    let mut score = 0usize;
    let mut gravity_ms = TICK_MS;

    'game: loop {
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Left => {
                        let mut moved = current.clone();
                        moved.x -= 1;
                        if !collides(&board, &moved) {
                            current = moved;
                        }
                    }
                    KeyCode::Right => {
                        let mut moved = current.clone();
                        moved.x += 1;
                        if !collides(&board, &moved) {
                            current = moved;
                        }
                    }
                    KeyCode::Down => {
                        let mut moved = current.clone();
                        moved.y += 1;
                        if !collides(&board, &moved) {
                            current = moved;
                            score += 1;
                        }
                    }
                    KeyCode::Up => {
                        let mut rotated = current.clone();
                        rotated.rotate_cw();
                        for k in [0, -1, 1, -2, 2] {
                            let mut t = rotated.clone();
                            t.x += k;
                            if !collides(&board, &t) {
                                current = t;
                                break;
                            }
                        }
                    }
                    KeyCode::Char(' ') => {
                        loop {
                            let mut moved = current.clone();
                            moved.y += 1;
                            if collides(&board, &moved) {
                                break;
                            }
                            current = moved;
                            score += 2;
                        }
                        lock_piece(&mut board, &current);
                        let cleared = clear_lines(&mut board);
                        if cleared > 0 {
                            score += 100 * (1 << (cleared - 1));
                        }
                        current = next;
                        next = Piece::random_spawn();
                        if collides(&board, &current) {
                            break 'game;
                        }
                    }
                    KeyCode::Char('q') | KeyCode::Esc => break 'game,
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= Duration::from_millis(gravity_ms) {
            last_tick = Instant::now();
            let mut moved = current.clone();
            moved.y += 1;
            if collides(&board, &moved) {
                lock_piece(&mut board, &current);
                let cleared = clear_lines(&mut board);
                if cleared > 0 {
                    score += 100 * (1 << (cleared - 1));
                }
                current = next;
                next = Piece::random_spawn();
                if collides(&board, &current) {
                    break 'game;
                }
                gravity_ms = max(100, TICK_MS.saturating_sub((score / 500) as u64 * 20));
            } else {
                current = moved;
            }
        }

        draw_board(stdout, &board, &current, &next, score)?;
        thread::sleep(Duration::from_millis(8));
    }

    stdout.queue(cursor::MoveTo(0, (HEIGHT + 10) as u16))?;
    writeln!(stdout, "💀 Game Over! Final score: {}", score)?;
    writeln!(stdout, "Press any key to exit...")?;
    stdout.flush()?;
    while !event::poll(Duration::from_millis(100))? {}
    if let Event::Key(_) = event::read()? {}
    Ok(())
}


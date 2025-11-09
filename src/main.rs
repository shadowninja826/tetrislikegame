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

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    style::{self, Color, PrintStyledContent, Stylize},
    terminal::{self, ClearType},
    ExecutableCommand, QueueableCommand,
    execute, // ← add this line
};

use rand::seq::SliceRandom;
use rand::thread_rng;
use std::cmp::max;
use std::io::{stdout, Write};
use std::time::{Duration, Instant};
use std::{thread, usize};
use std::io::Cursor;
use rodio::{Decoder, OutputStream, Sink};


fn play_music() {
    use std::thread;
    use std::io::Cursor;
    use rodio::{Decoder, OutputStream, Sink, Source};

    thread::spawn(|| {
        let bytes = include_bytes!("../assets/791018.mp3");
        let cursor = Cursor::new(bytes);

        if let Ok((_stream, handle)) = OutputStream::try_default() {
            if let Ok(source) = Decoder::new(cursor) {
                let sink = Sink::try_new(&handle).unwrap();
                sink.append(source.repeat_infinite());
                sink.sleep_until_end(); // keep playing forever
            } else {
                eprintln!("⚠️ Could not decode MP3 file.");
            }
        } else {
            eprintln!("⚠️ No audio output stream found (ALSA/PulseAudio missing?)");
        }
    });
}
/*
fn play_music() {
    // Embed MP3 file directly into the compiled binary
    let bytes = include_bytes!("../assets/791018.mp3");
    let cursor = Cursor::new(bytes);

    if let Ok((_stream, handle)) = OutputStream::try_default() {
        if let Ok(decoder) = Decoder::new_looped(cursor) {
            let sink = Sink::try_new(&handle).unwrap();
            sink.append(decoder);
            sink.detach(); // plays in background while game runs
        }
    }
}
*/

// Board size config — like life, keep it within boundaries
const WIDTH: usize = 10;
const HEIGHT: usize = 20;
const TICK_MS: u64 = 500; // gravity tick in milliseconds

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Empty,
    Filled(u8), // id for color/piece type
}

type Board = [[Cell; WIDTH]; HEIGHT];

#[derive(Clone)]
struct Piece {
    blocks: Vec<(i32, i32)>, // relative coords
    x: i32,                  // top-left offset in board
    y: i32,
    id: u8,
}

impl Piece {
    fn shape_rotations(name: usize) -> Vec<Vec<(i32, i32)>> {
        // Basic 7 tetromino shapes
        // Because rectangles are boring, and squares are overachievers.
        let shapes0: Vec<Vec<(i32, i32)>> = vec![
            vec![(-2, 0), (-1, 0), (0, 0), (1, 0)],          // I
            vec![(0, 0), (1, 0), (0, 1), (1, 1)],            // O
            vec![(-1, 0), (0, 0), (1, 0), (0, 1)],           // T
            vec![(-1, 0), (0, 0), (1, 0), (1, 1)],           // J
            vec![(-1, 0), (0, 0), (1, 0), (-1, 1)],          // L
            vec![(-1, 1), (0, 1), (0, 0), (1, 0)],           // S
            vec![(-1, 0), (0, 0), (0, 1), (1, 1)],           // Z
        ];

        let base = &shapes0[name];
        let mut rots = vec![];
        let mut current = base.clone();
        for _ in 0..4 {
            rots.push(current.clone());
            // rotate 90° clockwise — geometry class finally pays off
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
        1 => Color::Cyan,    // I
        2 => Color::Yellow,  // O
        3 => Color::Magenta, // T
        4 => Color::Blue,    // J
        5 => Color::Red,     // L
        6 => Color::Green,   // S
        7 => Color::DarkRed, // Z
        _ => Color::White,
    }
}

fn draw_board(stdout: &mut impl Write, board: &Board, piece: &Piece, score: usize) -> crossterm::Result<()> {
    stdout.queue(cursor::Hide)?;
    stdout.queue(terminal::Clear(ClearType::All))?;

    // Draw top border — like a hat for your game
    stdout.queue(cursor::MoveTo(0, 0))?;
    writeln!(stdout, "+{}+", "-".repeat(WIDTH))?;

    for r in 0..HEIGHT {
        stdout.queue(cursor::MoveTo(0, (r + 1) as u16))?;
        write!(stdout, "|")?;
        for c in 0..WIDTH {
            match board[r][c] {
                Cell::Empty => write!(stdout, " ")?,
                Cell::Filled(id) => {
                    let symbol = "█".with(piece_color(id));
                    stdout.queue(PrintStyledContent(symbol))?;
                }
            }
        }
        writeln!(stdout, "|")?;
    }

    stdout.queue(cursor::MoveTo(0, (HEIGHT + 1) as u16))?;
    writeln!(stdout, "+{}+", "-".repeat(WIDTH))?;

    // Overlay current piece
    for (x, y) in piece.positions() {
        if y >= 0 && y < HEIGHT as i32 && x >= 0 && x < WIDTH as i32 {
            stdout.queue(cursor::MoveTo((1 + x) as u16, (1 + y) as u16))?;
            let symbol = "▒".with(piece_color(piece.id));
            stdout.queue(PrintStyledContent(symbol))?;
        }
    }

    // Stats section
    let info_y = (HEIGHT + 3) as u16;
    stdout.queue(cursor::MoveTo(0, info_y))?;
    writeln!(stdout, "Score: {}", score)?;
    writeln!(stdout, "Controls: ← → ↓ ↑ Space  Q")?;
    writeln!(stdout, "Pro tip: If you lose, blame RNG.")?;

    stdout.flush()?;
    Ok(())
}

fn main() -> crossterm::Result<()> {
    play_music(); // 🎵 start looping cool tetris song assets/791018.mp3
    let mut stdout = stdout();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide).unwrap();
    terminal::enable_raw_mode().unwrap();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;
    stdout.execute(cursor::Hide)?;
    let res = run_game(&mut stdout);
    stdout.execute(cursor::Show)?;
    stdout.execute(terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    res
}

fn run_game(stdout: &mut impl Write) -> crossterm::Result<()> {
    let mut board = create_empty_board();
    let mut current = Piece::random_spawn();
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
                        current = Piece::random_spawn();
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
                current = Piece::random_spawn();
                if collides(&board, &current) {
                    break;
                }
                gravity_ms = max(100, TICK_MS.saturating_sub((score / 500) as u64 * 20));
            } else {
                current = moved;
            }
        }

        draw_board(stdout, &board, &current, score)?;
        thread::sleep(Duration::from_millis(8));
    }

    // Game over — don’t cry, just blame lag.
    stdout.queue(cursor::MoveTo(0, (HEIGHT + 6) as u16))?;
    writeln!(stdout, "💀 Game Over! Final score: {}", score)?;
    writeln!(stdout, "Press any key to exit...")?;
    stdout.flush()?;
    while !event::poll(Duration::from_millis(100))? {}
    if let Event::Key(_) = event::read()? {}
    Ok(())
}

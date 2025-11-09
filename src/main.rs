use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    terminal::{self, ClearType},
    ExecutableCommand, QueueableCommand,
};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::cmp::{max, min};
use std::io::{stdout, Write};
use std::time::{Duration, Instant};
use std::{thread, usize};

const WIDTH: usize = 10;
const HEIGHT: usize = 20;
const TICK_MS: u64 = 500; // gravity tick (ms). lower = faster

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Empty,
    Filled(u8), // id for color/piece
}

type Board = [[Cell; WIDTH]; HEIGHT];

#[derive(Clone)]
struct Piece {
    blocks: Vec<(i32, i32)>, // relative coords
    x: i32,                  // left/top offset in board coords
    y: i32,
    id: u8,
}

impl Piece {
    fn shape_rotations(name: usize) -> Vec<Vec<(i32, i32)>> {
        // Returns 4 rotations (0..3) for 7 tetromino shapes
        // Shapes defined in their 0-rotation. We'll pre-calc rotation by 90 degree turns about (0,0).
        // Order: I, O, T, J, L, S, Z
        let shapes0: Vec<Vec<(i32, i32)>> = vec![
            // I (horizontal)
            vec![(-2, 0), (-1, 0), (0, 0), (1, 0)],
            // O
            vec![(0, 0), (1, 0), (0, 1), (1, 1)],
            // T
            vec![(-1, 0), (0, 0), (1, 0), (0, 1)],
            // J
            vec![(-1, 0), (0, 0), (1, 0), (1, 1)],
            // L
            vec![(-1, 0), (0, 0), (1, 0), (-1, 1)],
            // S
            vec![(-1, 1), (0, 1), (0, 0), (1, 0)],
            // Z
            vec![(-1, 0), (0, 0), (0, 1), (1, 1)],
        ];

        let base = &shapes0[name];
        // compute 4 rotations
        let mut rots = vec![];
        let mut current = base.clone();
        for _ in 0..4 {
            rots.push(current.clone());
            // rotate 90 degrees: (x, y) -> (y, -x)
            current = current.iter().map(|(x, y)| (*y, -*x)).collect();
            // normalize so coordinates are small; not strictly necessary
        }
        rots
    }

    fn random_spawn() -> Self {
        let mut rng = thread_rng();
        let idx = (0..7).collect::<Vec<_>>().choose(&mut rng).cloned().unwrap();
        let rots = Piece::shape_rotations(idx);
        // start with 0 rotation
        let blocks = rots[0].clone();
        // spawn near top center
        let x = (WIDTH as i32 / 2) as i32;
        let y = 0;
        Piece {
            blocks,
            x,
            y,
            id: (idx + 1) as u8,
        }
    }

    fn rotate(&mut self) {
        // rotate relative coords 90deg CCW: (x,y) -> (y,-x)
        self.blocks = self
            .blocks
            .iter()
            .map(|(x, y)| (*y, -*x))
            .collect::<Vec<_>>();
    }

    fn rotate_cw(&mut self) {
        // clockwise: (x,y) -> (-y,x)
        self.blocks = self
            .blocks
            .iter()
            .map(|(x, y)| (-*y, *x))
            .collect::<Vec<_>>();
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
            continue; // skip copying this full row
        }
        // copy row down to write_row
        if write_row != read_row {
            for c in 0..WIDTH {
                board[write_row as usize][c] = board[read_row as usize][c];
            }
        }
        write_row -= 1;
    }

    // fill the remaining top rows with Empty
    for r in 0..=write_row {
        for c in 0..WIDTH {
            board[r as usize][c] = Cell::Empty;
        }
    }
    cleared
}

fn draw_board(stdout: &mut impl Write, board: &Board, piece: &Piece, score: usize) -> crossterm::Result<()> {
    stdout.queue(cursor::Hide)?;
    stdout.queue(terminal::Clear(ClearType::All))?;

    // top border
    stdout.queue(cursor::MoveTo(0, 0))?;
    writeln!(stdout, "+{}+", "-".repeat(WIDTH))?;
    // rows
    for r in 0..HEIGHT {
        stdout.queue(cursor::MoveTo(0, (r + 1) as u16))?;
        write!(stdout, "|")?;
        for c in 0..WIDTH {
            let mut ch = ' ';
            if let Cell::Filled(_) = board[r][c] {
                ch = '█';
            }
            write!(stdout, "{}", ch)?;
        }
        writeln!(stdout, "|")?;
    }
    // bottom border
    stdout.queue(cursor::MoveTo(0, (HEIGHT + 1) as u16))?;
    writeln!(stdout, "+{}+", "-".repeat(WIDTH))?;

    // overlay current piece
    for (x, y) in piece.positions() {
        if y >= 0 && y < HEIGHT as i32 && x >= 0 && x < WIDTH as i32 {
            stdout.queue(cursor::MoveTo((1 + x) as u16, (1 + y) as u16))?;
            write!(stdout, "▒")?;
        }
    }

    // score and controls
    let info_y = (HEIGHT + 3) as u16;
    stdout.queue(cursor::MoveTo(0, info_y))?;
    writeln!(stdout, "Score: {}", score)?;
    writeln!(stdout, "Controls: ← →  move | ↓ soft drop | ↑ rotate | space hard drop | q quit")?;

    stdout.flush()?;
    Ok(())
}

fn main() -> crossterm::Result<()> {
    let mut stdout = stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;
    stdout.execute(cursor::Hide)?;
    let res = run_game(&mut stdout);
    // restore terminal
    stdout.execute(cursor::Show)?;
    stdout.execute(terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    res
}

fn run_game(stdout: &mut impl Write) -> crossterm::Result<()> {
    let mut board = create_empty_board();
    let mut current = Piece::random_spawn();
    current.y = -2; // start slightly above visible area
    // sanity: if immediate collision when spawn => game over
    if collides(&board, &current) {
        // immediate game over
        draw_board(stdout, &board, &current, 0)?;
        return Ok(());
    }

    let mut last_tick = Instant::now();
    let mut score = 0usize;
    let mut gravity_ms = TICK_MS;
    let mut rng = thread_rng();

    'game: loop {
        // input handling non-blocking: poll with small timeout
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
                        // soft drop
                        let mut moved = current.clone();
                        moved.y += 1;
                        if !collides(&board, &moved) {
                            current = moved;
                            score += 1;
                        }
                    }
                    KeyCode::Up => {
                        // rotate cw with simple wall-kick attempt
                        let mut rotated = current.clone();
                        rotated.rotate_cw();
                        // try offsets 0, -1, +1, -2, +2
                        let kicks = [0, -1, 1, -2, 2];
                        let mut applied = false;
                        for k in kicks {
                            let mut t = rotated.clone();
                            t.x += k;
                            if !collides(&board, &t) {
                                current = t;
                                applied = true;
                                break;
                            }
                        }
                        if !applied {
                            // rotation failed; keep previous
                        }
                    }
                    KeyCode::Char(' ') => {
                        // hard drop
                        loop {
                            let mut moved = current.clone();
                            moved.y += 1;
                            if collides(&board, &moved) {
                                break;
                            }
                            current = moved;
                            score += 2;
                        }
                        // lock immediately
                        lock_piece(&mut board, &current);
                        let cleared = clear_lines(&mut board);
                        if cleared > 0 {
                            // scoring: 100 * (2^(cleared-1)) typical Tetris-like scale
                            score += 100 * (1 << (cleared - 1));
                        }
                        // spawn new
                        current = Piece::random_spawn();
                        current.y = -2;
                        if collides(&board, &current) {
                            // game over
                            break 'game;
                        }
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        break 'game;
                    }
                    _ => {}
                }
            }
        }

        // gravity tick
        if last_tick.elapsed() >= Duration::from_millis(gravity_ms) {
            last_tick = Instant::now();
            let mut moved = current.clone();
            moved.y += 1;
            if collides(&board, &moved) {
                // lock piece and spawn new
                lock_piece(&mut board, &current);
                let cleared = clear_lines(&mut board);
                if cleared > 0 {
                    score += 100 * (1 << (cleared - 1));
                }
                current = Piece::random_spawn();
                current.y = -2;
                if collides(&board, &current) {
                    // game over
                    break;
                }
                // speed up slightly as score increases (optional)
                gravity_ms = max(100, TICK_MS.saturating_sub((score / 500) as u64 * 20));
            } else {
                current = moved;
            }
        }

        // draw
        draw_board(stdout, &board, &current, score)?;

        // small sleep so CPU doesn't spin too hard
        thread::sleep(Duration::from_millis(8));
    }

    // Game over screen
    stdout.queue(cursor::MoveTo(0, (HEIGHT + 6) as u16))?;
    writeln!(stdout, "Game Over! Final score: {}", score)?;
    writeln!(stdout, "Press any key to exit...")?;
    stdout.flush()?;
    // wait for a key
    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(_) = event::read()? {
                break;
            }
        }
    }

    Ok(())
}

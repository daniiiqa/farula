//! El arte de "actualizar discada", en la terminal: la discada en la lumbre
//! meneada con la cuchara (`farula meneo`) y un vaquero bailando cuando queda
//! lista (`farula lista`). Los colores son los 16 de la terminal (los del look).
//! Solo se mueve en una terminal de verdad y sin DISCADA_SIN_ARTE; si no, un
//! renglón y ya.

use std::io::Write;
use std::time::Duration;

const FPS: u64 = 8;
/// ancho del dibujo
const W: usize = 34;

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Plain,
    Steam,
    Spoon,
    Rim,
    Food,
    Fat,
    Fire,
    Fire2,
    Text,
}

impl Tone {
    fn sgr(self) -> &'static str {
        match self {
            Tone::Plain => "",
            Tone::Steam => "\x1b[2;37m",
            Tone::Spoon => "\x1b[1;33m",
            Tone::Rim => "\x1b[1;37m",
            Tone::Food => "\x1b[0;31m",
            Tone::Fat => "\x1b[0;33m",
            Tone::Fire => "\x1b[1;31m",
            Tone::Fire2 => "\x1b[1;33m",
            Tone::Text => "\x1b[1;35m",
        }
    }
}

type Cell = (char, Tone);

fn paint(cells: &[Cell]) -> String {
    let mut out = String::new();
    let mut tone = Tone::Plain;
    for &(ch, t) in cells {
        if t != tone {
            out.push_str("\x1b[0m");
            out.push_str(t.sgr());
            tone = t;
        }
        out.push(ch);
    }
    out.push_str("\x1b[0m");
    out
}

fn text_row(s: &str, tone: Tone) -> Vec<Cell> {
    s.chars().map(|c| (c, tone)).collect()
}

/// Idioma de discada (hypr/discada.conf → LANGUAGE=es|en).
fn spanish() -> bool {
    let f = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/hypr/discada.conf");
    std::fs::read_to_string(f)
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("LANGUAGE=").map(|v| v.trim_start().starts_with("es"))))
        .unwrap_or(false)
}

/// El vapor: cuatro versiones que se turnan (suben un renglón cada dos cuadros).
const STEAM: [&str; 4] = [
    "      (    )        (    )      ",
    "    )    (     (       )    (   ",
    "      (      )    (     )       ",
    "   (     )      )    (      )   ",
];

/// La discada de lado: la orilla de la col 2 a la 25, la carne adentro (3–24)
/// dando vueltas, la cuchara entrando de arriba a la derecha con la punta
/// yendo y viniendo, el vapor, las patas y la lumbre.
fn meneo_frame(frame: usize, es: bool) -> Vec<String> {
    const PATH: [usize; 10] = [6, 9, 12, 15, 18, 21, 18, 15, 12, 9];
    let tip = PATH[frame % PATH.len()];
    let mut grid = vec![vec![(' ', Tone::Plain); W]; 8];
    let mut put = |r: usize, c: usize, ch: char, t: Tone| {
        if r < grid.len() && c < W {
            grid[r][c] = (ch, t);
        }
    };
    // vapor (renglones 0–2), subiendo
    for r in 0..3 {
        let line = STEAM[(frame / 2 + 2 - r) % STEAM.len()];
        for (c, ch) in line.chars().enumerate().take(W) {
            if ch != ' ' && (3..=24).contains(&c) {
                put(r, c, ch, Tone::Steam);
            }
        }
    }
    // la orilla (3), la carne (4) y el fondo (5)
    for c in 2..26 {
        put(3, c, '_', Tone::Rim);
    }
    put(4, 2, '\\', Tone::Rim);
    put(4, 25, '/', Tone::Rim);
    let food: Vec<char> = "~°~ ·~ ~°~ ·~ ~°~ ·~ ~°".chars().collect();
    let shift = frame % food.len();
    for k in 0..22 {
        let ch = food[(k + shift) % food.len()];
        put(4, 3 + k, ch, if matches!(ch, '°' | '·') { Tone::Food } else { Tone::Fat });
    }
    put(5, 3, '\\', Tone::Rim);
    for c in 4..24 {
        put(5, c, '_', Tone::Rim);
    }
    put(5, 24, '/', Tone::Rim);
    // la cuchara: la punta en la carne, el mango sube a la derecha
    put(4, tip, 'U', Tone::Spoon);
    for k in 1..5 {
        put(4 - k, tip + k, '/', Tone::Spoon);
    }
    // las patas y la lumbre
    put(6, 6, '|', Tone::Rim);
    put(6, 21, '|', Tone::Rim);
    let flames: Vec<char> = "^ ^^ ^ ^^^ ^ ^^ ^".chars().collect();
    for k in 0..14 {
        let ch = flames[(k + frame % 3) % flames.len()];
        if ch != ' ' {
            put(7, 7 + k, ch, if (k + frame) % 2 == 1 { Tone::Fire } else { Tone::Fire2 });
        }
    }
    let mut rows: Vec<String> = grid.iter().map(|r| paint(r)).collect();
    let label = if es { "   meneando la discada…" } else { "   stirring the discada…" };
    rows.push(paint(&text_row(label, Tone::Text)));
    rows
}

const DANCE: [[&str; 3]; 4] = [
    ["   \\o/  ", "    |   ", "   / \\  "],
    ["    o   ", "   /|\\  ", "   / \\  "],
    ["    o/  ", "   /|   ", "   / >  "],
    ["   \\o   ", "    |\\  ", "   < \\  "],
];

/// El vaquero (sombrero y todo) bailando entre notas.
fn lista_frame(frame: usize, es: bool) -> Vec<String> {
    let d = DANCE[frame % DANCE.len()];
    let pad = " ".repeat(10);
    let notes = if frame % 2 == 1 { "    ♪                   ♫ " } else { "     ♫                   ♪" };
    let label = if es { "   ¡discada lista!" } else { "   discada is ready!" };
    vec![
        paint(&text_row(&format!("{pad}   _n_  "), Tone::Spoon)),
        paint(&text_row(&format!("{pad}{}", d[0]), Tone::Rim)),
        paint(&text_row(&format!("{pad}{}", d[1]), Tone::Rim)),
        paint(&text_row(&format!("{pad}{}", d[2]), Tone::Rim)),
        paint(&text_row(notes, Tone::Fat)),
        paint(&text_row(label, Tone::Text)),
    ]
}

fn columns() -> usize {
    // SAFETY: TIOCGWINSZ fills a winsize; on failure we keep the default
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            return ws.ws_col as usize;
        }
    }
    80
}

extern "C" fn restore_cursor(_: libc::c_int) {
    const SHOW: &[u8] = b"\x1b[?25h\x1b[0m\n";
    // SAFETY: write and _exit are async-signal-safe
    unsafe {
        libc::write(libc::STDOUT_FILENO, SHOW.as_ptr().cast(), SHOW.len());
        libc::_exit(130);
    }
}

fn play(make: fn(usize, bool) -> Vec<String>, seconds: f64) {
    let es = spanish();
    // SAFETY: isatty only inspects the fd
    let tty = unsafe { libc::isatty(libc::STDOUT_FILENO) } == 1;
    if !tty || std::env::var_os("DISCADA_SIN_ARTE").is_some() || std::env::var("TERM").as_deref() == Ok("dumb") {
        if tty {
            // the last row is the label; without colors
            let label = make(0, es).pop().unwrap_or_default();
            let plain: String = strip_sgr(&label);
            println!("{}", plain.trim());
        }
        return;
    }
    // SAFETY: installing a handler that only uses async-signal-safe calls
    unsafe {
        libc::signal(libc::SIGINT, restore_cursor as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, restore_cursor as *const () as libc::sighandler_t);
    }
    let pad = " ".repeat(columns().saturating_sub(W) / 2);
    let height = make(0, es).len();
    let frames = ((seconds * FPS as f64) as usize).max(1);
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "\x1b[?25l"); // sin cursor mientras se mueve
    for i in 0..frames {
        if i > 0 {
            let _ = write!(out, "\x1b[{height}F"); // al principio del dibujo
        }
        for line in make(i, es) {
            let _ = writeln!(out, "\x1b[2K{pad}{line}");
        }
        let _ = out.flush();
        std::thread::sleep(Duration::from_millis(1000 / FPS));
    }
    let _ = write!(out, "\x1b[?25h");
    let _ = out.flush();
}

fn strip_sgr(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `farula meneo [segundos]` (2.5 s) / `farula lista [segundos]` (2 s).
pub fn run(what: &str, seconds: Option<&str>) -> i32 {
    let secs = seconds.and_then(|s| s.parse::<f64>().ok());
    match what {
        "meneo" => play(meneo_frame, secs.unwrap_or(2.5)),
        _ => play(lista_frame, secs.unwrap_or(2.0)),
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_keep_their_shape() {
        for f in 0..20 {
            let rows = meneo_frame(f, false);
            assert_eq!(rows.len(), 9);
            for r in &rows[..8] {
                assert_eq!(strip_sgr(r).chars().count(), W);
            }
            // the spoon is in the pot on every frame
            assert!(strip_sgr(&rows[4]).contains('U'));
            assert_eq!(lista_frame(f, true).len(), 6);
        }
        assert!(strip_sgr(&lista_frame(0, true)[5]).contains("¡discada lista!"));
    }
}

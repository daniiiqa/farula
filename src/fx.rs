//! Los efectos con que se va la foto vieja al revelar. Farula solo tiene la
//! foto de la pantalla de antes (la nueva es la de verdad, debajo), así que
//! cada efecto es una forma de quitarla: recortarla, quemarla, romperla,
//! derretirla. Todo en BGRA premultiplicado, en CPU; lo caro (el ruido del
//! quemado, los pedazos del vidrio) se calcula una vez al empezar.
//!
//! En el look.toml:   [farula]  fx = "quemado"   (o una lista, o "random")

use crate::paint::{self, Frame};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fx {
    Iris,
    Quemado,
    Glitch,
    Vidrio,
    Derretido,
    Fade,
    Crt,
    Azulejos,
    Cartas,
    Onda,
    Tinta,
    Wipe,
    Pixelado,
    Humo,
}

pub const ALL: [Fx; 14] = [
    Fx::Iris,
    Fx::Quemado,
    Fx::Glitch,
    Fx::Vidrio,
    Fx::Derretido,
    Fx::Fade,
    Fx::Crt,
    Fx::Azulejos,
    Fx::Cartas,
    Fx::Onda,
    Fx::Tinta,
    Fx::Wipe,
    Fx::Pixelado,
    Fx::Humo,
];

impl Fx {
    pub fn parse(s: &str) -> Option<Fx> {
        Some(match s.trim().trim_matches('"').to_lowercase().as_str() {
            "iris" => Fx::Iris,
            "quemado" | "burn" => Fx::Quemado,
            "glitch" => Fx::Glitch,
            "vidrio" | "shatter" => Fx::Vidrio,
            "derretido" | "melt" => Fx::Derretido,
            "fade" => Fx::Fade,
            "crt" | "apagon" | "apagón" => Fx::Crt,
            "azulejos" | "tiles" => Fx::Azulejos,
            "cartas" | "cards" => Fx::Cartas,
            "onda" | "ripple" => Fx::Onda,
            "tinta" | "ink" => Fx::Tinta,
            "wipe" | "barrida" => Fx::Wipe,
            "pixelado" | "pixel" => Fx::Pixelado,
            "humo" | "smoke" => Fx::Humo,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Fx::Iris => "iris",
            Fx::Quemado => "quemado",
            Fx::Glitch => "glitch",
            Fx::Vidrio => "vidrio",
            Fx::Derretido => "derretido",
            Fx::Fade => "fade",
            Fx::Crt => "crt",
            Fx::Azulejos => "azulejos",
            Fx::Cartas => "cartas",
            Fx::Onda => "onda",
            Fx::Tinta => "tinta",
            Fx::Wipe => "wipe",
            Fx::Pixelado => "pixelado",
            Fx::Humo => "humo",
        }
    }
    /// Cuánto dura el reveal (segundos).
    pub fn secs(self) -> f32 {
        match self {
            Fx::Iris => 1.1,
            Fx::Quemado => 1.3,
            Fx::Glitch => 0.9,
            Fx::Vidrio => 1.5,
            Fx::Derretido => 1.4,
            Fx::Fade => 0.45,
            Fx::Crt => 0.8,
            Fx::Azulejos => 1.3,
            Fx::Cartas => 1.5,
            Fx::Onda => 1.2,
            Fx::Tinta => 1.4,
            Fx::Wipe => 0.7,
            Fx::Pixelado => 1.0,
            Fx::Humo => 2.0,
        }
    }
}

/// Un cuadro del reveal, en pixeles del buffer.
pub struct Scene<'a> {
    pub base: &'a [u8],
    pub w: usize,
    pub h: usize,
    pub cx: f32,
    pub cy: f32,
    /// hasta la esquina más lejana de todos los monitores
    pub far: f32,
    /// pixeles del buffer por pixel lógico
    pub sf: f32,
    /// 0..1 del reveal
    pub t: f32,
    pub dim: f32,
    pub accent: [f32; 3],
    pub seed: u32,
}

/// Lo que cada efecto precalcula por monitor.
pub enum Data {
    Nada,
    Quemado(Vec<u16>),
    Vidrio(Vidrio),
    Derretido(Vec<f32>),
    Tinta(Vec<u16>),
    Humo(Humo),
}

pub fn prepare(fx: Fx, s: &Scene) -> Data {
    match fx {
        Fx::Quemado => Data::Quemado(burn_field(s)),
        Fx::Vidrio => Data::Vidrio(Vidrio::new(s)),
        Fx::Derretido => Data::Derretido(melt_delays(s)),
        Fx::Tinta => Data::Tinta(ink_field(s)),
        Fx::Humo => Data::Humo(Humo::new(s)),
        _ => Data::Nada,
    }
}

pub fn paint(fx: Fx, data: &Data, canvas: &mut [u8], s: &Scene) {
    if s.t >= 1.0 {
        canvas.fill(0);
        return;
    }
    match (fx, data) {
        (Fx::Quemado, Data::Quemado(n)) => burn(canvas, s, n),
        (Fx::Glitch, _) => glitch(canvas, s),
        (Fx::Vidrio, Data::Vidrio(v)) => v.paint(canvas, s),
        (Fx::Derretido, Data::Derretido(d)) => melt(canvas, s, d),
        (Fx::Fade, _) => fade(canvas, s),
        (Fx::Crt, _) => crt(canvas, s),
        (Fx::Azulejos, _) => azulejos(canvas, s),
        (Fx::Cartas, _) => cartas(canvas, s),
        (Fx::Onda, _) => onda(canvas, s),
        (Fx::Tinta, Data::Tinta(f)) => tinta(canvas, s, f),
        (Fx::Wipe, _) => wipe(canvas, s),
        (Fx::Pixelado, _) => pixelado(canvas, s),
        (Fx::Humo, Data::Humo(h)) => h.paint(canvas, s),
        _ => iris(canvas, s),
    }
}

// ─── utilidades ─────────────────────────────────────────────────────────────

pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

fn rand01(x: u32) -> f32 {
    hash(x) as f32 / u32::MAX as f32
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

fn ease_expo(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 || t == 1.0 {
        t
    } else if t < 0.5 {
        2f32.powf(20.0 * t - 10.0) / 2.0
    } else {
        (2.0 - 2f32.powf(-20.0 * t + 10.0)) / 2.0
    }
}

/// Ruido de valor 2D, 0..1.
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (smooth(x - ix), smooth(y - iy));
    let (ix, iy) = (ix as i32 as u32, iy as i32 as u32);
    let at = |i: u32, j: u32| rand01(i.wrapping_mul(73_856_093) ^ j.wrapping_mul(19_349_663) ^ seed);
    let a = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * fx;
    let b = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * fx;
    a + (b - a) * fy
}

fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..5 {
        sum += noise(x * freq, y * freq, seed.wrapping_add(o * 1013)) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Escribe un pixel premultiplicado (canales 0..1, en RGB) como BGRA.
#[inline]
fn put(d: &mut [u8], r: f32, g: f32, b: f32, a: f32) {
    d[0] = (b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    d[1] = (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    d[2] = (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    d[3] = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
}

/// El pixel viejo (BGRA) como RGB 0..1, ya oscurecido.
#[inline]
fn old(src: &[u8], k: f32) -> [f32; 3] {
    [src[2] as f32 / 255.0 * k, src[1] as f32 / 255.0 * k, src[0] as f32 / 255.0 * k]
}

/// El pixel viejo oscurecido, en enteros (lo que más se repite).
#[inline]
fn dim_px(d: &mut [u8], src: &[u8], k: u32) {
    d[0] = ((src[0] as u32 * k) >> 8) as u8;
    d[1] = ((src[1] as u32 * k) >> 8) as u8;
    d[2] = ((src[2] as u32 * k) >> 8) as u8;
    d[3] = 255;
}

/// Cada fila por su lado, repartidas entre los núcleos: `f(y, fila)`.
fn par_rows<F: Fn(usize, &mut [u8]) + Sync>(canvas: &mut [u8], w: usize, f: F) {
    let h = canvas.len() / (w * 4);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    let band = h.div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        for (b, rows) in canvas.chunks_mut(band * w * 4).enumerate() {
            let f = &f;
            sc.spawn(move || {
                for (i, row) in rows.chunks_exact_mut(w * 4).enumerate() {
                    f(b * band + i, row);
                }
            });
        }
    });
}

/// El pixel de la foto en (x, y), con las orillas pegadas.
#[inline]
fn at<'a>(s: &Scene<'a>, x: f32, y: f32) -> &'a [u8] {
    let x = (x as i32).clamp(0, s.w as i32 - 1) as usize;
    let y = (y as i32).clamp(0, s.h as i32 - 1) as usize;
    &s.base[(y * s.w + x) * 4..][..4]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Lo caliente: el acento del look, empujado hacia naranja para que se vea
/// fuego aunque la paleta sea gris.
fn fire(accent: [f32; 3]) -> [f32; 3] {
    mix(accent, [1.0, 0.45, 0.08], 0.55)
}

// ─── iris ───────────────────────────────────────────────────────────────────

/// orilla suave del iris y resplandor del anillo (pixeles lógicos)
pub const EDGE: f32 = 46.0;
pub const GLOW: f32 = 90.0;

fn iris(canvas: &mut [u8], s: &Scene) {
    let e = ease_expo(s.t);
    let far = s.far + (EDGE + GLOW) * s.sf;
    paint::paint(canvas, s.base, s.w, s.h, &Frame {
        cx: s.cx,
        cy: s.cy,
        r: e * far,
        edge: EDGE * s.sf,
        glow: GLOW * s.sf,
        dim: s.dim,
        ring: s.accent,
        ring_a: 0.85 * (1.0 - ((e - 0.75) * 4.0).max(0.0)),
    });
}

// ─── fade ───────────────────────────────────────────────────────────────────

fn fade(canvas: &mut [u8], s: &Scene) {
    let a = 1.0 - ease_in_out(s.t);
    let k = ((1.0 - s.dim) * a * 256.0) as u32;
    let al = (a * 255.0 + 0.5) as u8;
    for (d, src) in canvas.chunks_exact_mut(4).zip(s.base.chunks_exact(4)) {
        d[0] = ((src[0] as u32 * k) >> 8) as u8;
        d[1] = ((src[1] as u32 * k) >> 8) as u8;
        d[2] = ((src[2] as u32 * k) >> 8) as u8;
        d[3] = al;
    }
}

// ─── quemado ────────────────────────────────────────────────────────────────

/// Cuándo se quema cada pixel (0..1): ruido fractal más la distancia al
/// cursor, para que la lumbre arranque donde picaste. El ruido se calcula a
/// un cuarto de resolución y se interpola.
fn burn_field(s: &Scene) -> Vec<u16> {
    let n = noise_field(s, 260.0);
    let mut out = vec![0u16; s.w * s.h];
    for y in 0..s.h {
        for x in 0..s.w {
            let i = y * s.w + x;
            let d = ((x as f32 - s.cx).hypot(y as f32 - s.cy) / s.far).min(1.0);
            out[i] = ((0.55 * n[i] + 0.45 * d).clamp(0.0, 1.0) * 65535.0) as u16;
        }
    }
    out
}

/// Ruido fractal (0..1, con contraste) a resolución completa: se calcula a
/// un cuarto y se interpola. `size`: el tamaño de sus manchas (px lógicos).
fn noise_field(s: &Scene, size: f32) -> Vec<f32> {
    const STEP: usize = 4;
    let (lw, lh) = (s.w / STEP + 2, s.h / STEP + 2);
    let scale = 1.0 / (size * s.sf);
    let mut lo = vec![0f32; lw * lh];
    for j in 0..lh {
        for i in 0..lw {
            let n = fbm((i * STEP) as f32 * scale, (j * STEP) as f32 * scale, s.seed);
            lo[j * lw + i] = ((n - 0.5) * 2.4 + 0.5).clamp(0.0, 1.0);
        }
    }
    let mut out = vec![0f32; s.w * s.h];
    for y in 0..s.h {
        let fy = y as f32 / STEP as f32;
        let (j, ty) = (fy as usize, fy.fract());
        for x in 0..s.w {
            let fx = x as f32 / STEP as f32;
            let (i, tx) = (fx as usize, fx.fract());
            let a = lo[j * lw + i] + (lo[j * lw + i + 1] - lo[j * lw + i]) * tx;
            let b = lo[(j + 1) * lw + i] + (lo[(j + 1) * lw + i + 1] - lo[(j + 1) * lw + i]) * tx;
            out[y * s.w + x] = a + (b - a) * ty;
        }
    }
    out
}

fn burn(canvas: &mut [u8], s: &Scene, field: &[u16]) {
    const BAND: f32 = 0.045; // la brasa
    const CHAR: f32 = 0.07; // lo chamuscado antes de la brasa
    let e = ease_in_out(s.t);
    let th = -0.02 + e * (1.0 + BAND + 0.05);
    let k = 1.0 - s.dim;
    let ki = (k * 256.0) as u32;
    let hot = fire(s.accent);
    let core = [1.0, 0.95, 0.82];
    for ((d, src), &v) in canvas.chunks_exact_mut(4).zip(s.base.chunks_exact(4)).zip(field) {
        let v = v as f32 / 65535.0;
        if v < th - BAND {
            d.copy_from_slice(&[0, 0, 0, 0]);
        } else if v < th {
            // la brasa: blanca en el frente, del color del fuego hacia dentro
            let u = (th - v) / BAND;
            let a = (1.0 - u).powf(1.6);
            let c = mix(core, hot, (u * 2.0).min(1.0));
            put(d, c[0] * a, c[1] * a, c[2] * a, a);
        } else if v < th + CHAR {
            let f = (v - th) / CHAR;
            let o = old(src, k);
            let dark = 0.2 + 0.8 * f;
            let heat = (1.0 - f).powi(3) * 0.55;
            put(d, o[0] * dark + hot[0] * heat, o[1] * dark + hot[1] * heat, o[2] * dark + hot[2] * heat, 1.0);
        } else {
            dim_px(d, src, ki);
        }
    }
}

// ─── glitch ─────────────────────────────────────────────────────────────────

fn glitch(canvas: &mut [u8], s: &Scene) {
    let (w, sf) = (s.w, s.sf);
    let k = 1.0 - s.dim;
    // la señal cambia cada ~33 ms: rebanadas, saltos y separación RGB nuevos
    let q = (s.t * Fx::Glitch.secs() * 30.0) as u32 ^ s.seed;
    let slice_h = (((10 + hash(q) % 4 * 12) as f32 * sf) as usize).max(1);
    let gone_h = ((18.0 * sf) as usize).max(1);
    let split = ((4.0 + 22.0 * s.t) * sf * (0.5 + rand01(q ^ 0xabc))) as isize;
    let tt = s.t * 1.15 - 0.08;
    let scan = ((2.0 * sf) as usize).max(1);
    for y in 0..s.h {
        let row = &mut canvas[y * w * 4..(y + 1) * w * 4];
        // las franjas que ya se fueron: fijas, para que avance parejo; la
        // recién ida destella con el acento
        let gone = rand01(hash((y / gone_h) as u32 * 977 + s.seed));
        if gone < tt {
            if gone > tt - 0.06 {
                let a = s.accent;
                for d in row.chunks_exact_mut(4) {
                    put(d, a[0] * 0.9, a[1] * 0.9, a[2] * 0.9, 0.9);
                }
            } else {
                row.fill(0);
            }
            continue;
        }
        let sl = (y / slice_h) as u32;
        let r = rand01(hash(sl ^ q.wrapping_mul(7919)));
        let dx = if r < 0.15 + 0.45 * s.t {
            ((rand01(hash(sl.wrapping_mul(31) ^ q)) - 0.5) * 2.0 * 140.0 * sf * (0.4 + s.t)) as isize
        } else {
            0
        };
        let dim = if (y / scan) % 2 == 1 { k * 0.82 } else { k };
        let src = &s.base[y * w * 4..(y + 1) * w * 4];
        let at = |x: isize, c: usize| src[(x.clamp(0, w as isize - 1) as usize) * 4 + c] as f32 / 255.0 * dim;
        for (x, d) in row.chunks_exact_mut(4).enumerate() {
            let x = x as isize - dx;
            put(d, at(x + split, 2), at(x, 1), at(x - split, 0), 1.0);
        }
    }
}

// ─── vidrio ─────────────────────────────────────────────────────────────────

struct Piece {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    cx: f32,
    cy: f32,
    delay: f32,
    vx: f32,
    vy: f32,
    spin: f32,
}

/// La pantalla estrellada: cada pixel sabe de qué pedazo es (Voronoi sobre
/// una rejilla con semillas movidas; el bit alto marca las grietas).
pub struct Vidrio {
    id: Vec<u16>,
    pieces: Vec<Piece>,
}

const CRACK: u16 = 0x8000;
const CRACK_T: f32 = 0.15; // segundos que se ven las grietas antes de caer

impl Vidrio {
    fn new(s: &Scene) -> Vidrio {
        let (w, h) = (s.w, s.h);
        let cell = 150.0 * s.sf;
        let gw = (w as f32 / cell).ceil() as usize + 1;
        let gh = (h as f32 / cell).ceil() as usize + 1;
        let seeds: Vec<(f32, f32)> = (0..gw * gh)
            .map(|n| {
                let (i, j) = (n % gw, n / gw);
                let r = |k: u32| 0.12 + 0.76 * rand01(hash(n as u32 * 3 + k) ^ s.seed);
                ((i as f32 + r(0)) * cell, (j as f32 + r(1)) * cell)
            })
            .collect();
        let mut id = vec![0u16; w * h];
        for y in 0..h {
            let gj = (y as f32 / cell) as usize;
            for x in 0..w {
                let gi = (x as f32 / cell) as usize;
                let (mut best, mut bd) = (0usize, f32::MAX);
                for j in gj.saturating_sub(1)..=(gj + 1).min(gh - 1) {
                    for i in gi.saturating_sub(1)..=(gi + 1).min(gw - 1) {
                        let n = j * gw + i;
                        let (sx, sy) = seeds[n];
                        let d = (sx - x as f32).powi(2) + (sy - y as f32).powi(2);
                        if d < bd {
                            bd = d;
                            best = n;
                        }
                    }
                }
                id[y * w + x] = best as u16;
            }
        }
        // grietas de 2 px: los dos lados de cada orilla
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let me = id[i] & !CRACK;
                if x + 1 < w && id[i + 1] & !CRACK != me {
                    id[i] |= CRACK;
                    id[i + 1] |= CRACK;
                }
                if y + 1 < h && id[i + w] & !CRACK != me {
                    id[i] |= CRACK;
                    id[i + w] |= CRACK;
                }
            }
        }
        let mut pieces: Vec<Piece> = (0..gw * gh)
            .map(|_| Piece { x0: i32::MAX, y0: i32::MAX, x1: -1, y1: -1, cx: 0.0, cy: 0.0, delay: 0.0, vx: 0.0, vy: 0.0, spin: 0.0 })
            .collect();
        let mut count = vec![0u32; gw * gh];
        for y in 0..h {
            for x in 0..w {
                let n = (id[y * w + x] & !CRACK) as usize;
                let p = &mut pieces[n];
                p.x0 = p.x0.min(x as i32);
                p.y0 = p.y0.min(y as i32);
                p.x1 = p.x1.max(x as i32);
                p.y1 = p.y1.max(y as i32);
                p.cx += x as f32;
                p.cy += y as f32;
                count[n] += 1;
            }
        }
        for (n, p) in pieces.iter_mut().enumerate() {
            if count[n] == 0 {
                continue;
            }
            p.cx /= count[n] as f32;
            p.cy /= count[n] as f32;
            let (dx, dy) = (p.cx - s.cx, p.cy - s.cy);
            let len = dx.hypot(dy).max(1.0);
            let r = |k: u32| rand01(hash(n as u32 * 7 + k) ^ s.seed);
            // los cercanos al cursor caen primero, y salen hacia fuera
            p.delay = 0.38 * (len / s.far).min(1.0) + 0.05 * r(1);
            p.vx = dx / len * (150.0 + 250.0 * r(2)) * s.sf;
            p.vy = dy / len * (100.0 + 150.0 * r(3)) * s.sf - 260.0 * s.sf;
            p.spin = (r(4) - 0.5) * 4.0;
        }
        Vidrio { id, pieces }
    }

    fn paint(&self, canvas: &mut [u8], s: &Scene) {
        let (w, h) = (s.w, s.h);
        let tau = s.t * Fx::Vidrio.secs();
        let g = 2600.0 * s.sf;
        // dónde va cada pedazo que ya cae: corrimiento, giro y su caja en
        // pantalla (los que siguen en su lugar: None)
        let moves: Vec<Option<Move>> = self
            .pieces
            .iter()
            .map(|p| {
                let l = tau - CRACK_T - p.delay;
                if l <= 0.0 || p.x1 < 0 {
                    return None;
                }
                let (ox, oy) = (p.vx * l, p.vy * l + 0.5 * g * l * l);
                let (sin, cos) = (p.spin * l).sin_cos();
                let corners = [(p.x0, p.y0), (p.x1, p.y0), (p.x0, p.y1), (p.x1, p.y1)].map(|(x, y)| {
                    let (rx, ry) = (x as f32 - p.cx, y as f32 - p.cy);
                    (cos * rx - sin * ry + p.cx + ox, sin * rx + cos * ry + p.cy + oy)
                });
                let lo = |f: fn(&(f32, f32)) -> f32| corners.iter().map(f).fold(f32::MAX, f32::min).floor();
                let hi = |f: fn(&(f32, f32)) -> f32| corners.iter().map(f).fold(f32::MIN, f32::max).ceil();
                Some(Move {
                    ox,
                    oy,
                    sin,
                    cos,
                    bx0: lo(|c| c.0).max(0.0) as i32,
                    by0: lo(|c| c.1).max(0.0) as i32,
                    bx1: hi(|c| c.0).min(w as f32 - 1.0) as i32,
                    by1: hi(|c| c.1).min(h as f32 - 1.0) as i32,
                })
            })
            .collect();
        // cada hilo pinta una franja de filas: nadie escribe en la de otro
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
        let band = h.div_ceil(threads);
        std::thread::scope(|sc| {
            for (b, rows) in canvas.chunks_mut(band * w * 4).enumerate() {
                let moves = &moves;
                sc.spawn(move || self.paint_rows(rows, b * band, s, tau, moves));
            }
        });
    }

    /// Las filas desde `y0` (lo que cabe en `rows`).
    fn paint_rows(&self, rows: &mut [u8], y0: usize, s: &Scene, tau: f32, moves: &[Option<Move>]) {
        let w = s.w;
        let y1 = y0 + rows.len() / (w * 4);
        let k = 1.0 - s.dim;
        let ki = (k * 256.0) as u32;
        let crack_a = (tau / CRACK_T).min(1.0) * 0.75;
        // 1) los pedazos que siguen en su lugar, con las grietas brillando
        let base = &s.base[y0 * w * 4..y1 * w * 4];
        for (i, (d, src)) in rows.chunks_exact_mut(4).zip(base.chunks_exact(4)).enumerate() {
            let v = self.id[y0 * w + i];
            if moves[(v & !CRACK) as usize].is_some() {
                d.copy_from_slice(&[0, 0, 0, 0]);
            } else if v & CRACK != 0 {
                let o = mix(old(src, k), [1.0, 1.0, 1.0], crack_a);
                put(d, o[0], o[1], o[2], 1.0);
            } else {
                dim_px(d, src, ki);
            }
        }
        // 2) los que caen: girando sobre su centro, con gravedad; se apagan
        //    al final por si alguno no alcanzó a salir
        let alpha = ((Fx::Vidrio.secs() - tau) / 0.25).clamp(0.0, 1.0);
        for (n, (p, m)) in self.pieces.iter().zip(moves).enumerate() {
            let Some(m) = m else { continue };
            for y in m.by0.max(y0 as i32)..=m.by1.min(y1 as i32 - 1) {
                for x in m.bx0..=m.bx1 {
                    // de vuelta al pixel original (rotación inversa)
                    let (rx, ry) = (x as f32 + 0.5 - p.cx - m.ox, y as f32 + 0.5 - p.cy - m.oy);
                    let sx = (m.cos * rx + m.sin * ry + p.cx) as i32;
                    let sy = (-m.sin * rx + m.cos * ry + p.cy) as i32;
                    if sx < p.x0 || sx > p.x1 || sy < p.y0 || sy > p.y1 {
                        continue;
                    }
                    let si = sy as usize * w + sx as usize;
                    let v = self.id[si];
                    if (v & !CRACK) as usize != n {
                        continue;
                    }
                    let src = &s.base[si * 4..si * 4 + 4];
                    let d = &mut rows[((y as usize - y0) * w + x as usize) * 4..][..4];
                    if alpha >= 1.0 && v & CRACK == 0 {
                        dim_px(d, src, ki);
                        continue;
                    }
                    let o = old(src, k);
                    let o = if v & CRACK != 0 { mix(o, [1.0, 1.0, 1.0], 0.5) } else { o };
                    if alpha >= 1.0 {
                        put(d, o[0], o[1], o[2], 1.0);
                    } else {
                        let under = 1.0 - alpha;
                        let b = |c: usize| d[c] as f32 / 255.0 * under;
                        let (r0, g0, b0, a0) = (b(2), b(1), b(0), b(3));
                        put(d, o[0] * alpha + r0, o[1] * alpha + g0, o[2] * alpha + b0, alpha + a0);
                    }
                }
            }
        }
    }
}

/// Dónde va un pedazo en este cuadro.
struct Move {
    ox: f32,
    oy: f32,
    sin: f32,
    cos: f32,
    bx0: i32,
    by0: i32,
    bx1: i32,
    by1: i32,
}

// ─── derretido ──────────────────────────────────────────────────────────────

const MELT_COL: f32 = 6.0; // ancho de cada chorro (pixeles lógicos)

/// Cuándo empieza a escurrir cada columna: ruido suave (los vecinos se
/// parecen) más la distancia al cursor en x.
fn melt_delays(s: &Scene) -> Vec<f32> {
    let cw = (MELT_COL * s.sf).max(1.0);
    let n = (s.w as f32 / cw).ceil() as usize;
    (0..n)
        .map(|i| {
            let x = i as f32 * cw;
            // gotas anchas y parejas; un pelito de azar para que no sea liso
            let nz = 0.7 * noise(x / (170.0 * s.sf), 0.5, s.seed) + 0.3 * noise(x / (45.0 * s.sf), 3.5, s.seed);
            let r = rand01(hash(i as u32) ^ s.seed);
            0.5 * (0.62 * nz + 0.05 * r + 0.33 * ((x - s.cx).abs() / s.w as f32).min(1.0))
        })
        .collect()
}

fn melt(canvas: &mut [u8], s: &Scene, delays: &[f32]) {
    let (w, h) = (s.w, s.h);
    let cw = ((MELT_COL * s.sf) as usize).max(1);
    let k = 1.0 - s.dim;
    let ki = (k * 256.0) as u32;
    let molten = mix(fire(s.accent), [1.0, 0.9, 0.7], 0.3);
    let glow_h = 28.0 * s.sf;
    // cae acelerando: a t = 1 todas ya salieron (la que más tarda empieza en 0.5)
    let offs: Vec<f32> = delays.iter().map(|&dl| (s.t - dl).max(0.0).powi(2) * 4.6 * h as f32).collect();
    for y in 0..h {
        let row = &mut canvas[y * w * 4..(y + 1) * w * 4];
        for (c, &off) in offs.iter().enumerate() {
            let (x0, x1) = (c * cw, ((c + 1) * cw).min(w));
            let dst = &mut row[x0 * 4..x1 * 4];
            let sy = y as f32 - off;
            if sy < 0.0 {
                dst.fill(0);
                continue;
            }
            let sy = sy as usize;
            let src = &s.base[(sy * w + x0) * 4..(sy * w + x1) * 4];
            // la orilla de arriba del chorro, al rojo vivo
            let g = if off > 0.0 && (sy as f32) < glow_h {
                (1.0 - sy as f32 / glow_h).powi(2) * (off / (30.0 * s.sf)).min(1.0)
            } else {
                0.0
            };
            for (d, p) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                if g > 0.0 {
                    let o = mix(old(p, k), molten, g);
                    put(d, o[0], o[1], o[2], 1.0);
                } else {
                    dim_px(d, p, ki);
                }
            }
        }
    }
}

// ─── crt ────────────────────────────────────────────────────────────────────

/// Apagón de tele vieja: la imagen se aplasta a una raya en medio, la raya
/// se encoge a un punto y el punto se apaga.
fn crt(canvas: &mut [u8], s: &Scene) {
    let (w, h) = (s.w as f32, s.h as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let k = 1.0 - s.dim;
    let p1 = (s.t / 0.45).min(1.0);
    let p2 = ((s.t - 0.45) / 0.3).clamp(0.0, 1.0);
    let p3 = ((s.t - 0.75) / 0.25).clamp(0.0, 1.0);
    // se aplasta acelerando, y entre más se aplasta más brilla
    let hh = (h * (1.0 - p1 * p1 * p1)).max(2.0 * s.sf);
    let ww = w * (1.0 - p2 * p2);
    let bright = p1 * 0.85;
    let glow = 40.0 * s.sf;
    par_rows(canvas, s.w, |y, row| {
        let fy = y as f32 + 0.5;
        let dy = (fy - cy).abs();
        for (x, d) in row.chunks_exact_mut(4).enumerate() {
            if p2 <= 0.0 {
                if dy < hh / 2.0 {
                    let sy = (fy - (cy - hh / 2.0)) / hh * h;
                    let c = mix(old(at(s, x as f32, sy), k), [1.0, 1.0, 1.0], bright);
                    put(d, c[0], c[1], c[2], 1.0);
                } else {
                    let g = (1.0 - (dy - hh / 2.0) / glow).max(0.0).powi(2) * p1.powi(4) * 0.6;
                    put(d, g, g, g, g);
                }
            } else {
                // la raya (y luego el punto): blanca, con halo
                let ex = ((x as f32 + 0.5 - cx).abs() - ww / 2.0).max(0.0);
                let dist = ex.hypot(dy);
                let core = if dist < s.sf.max(1.0) { 1.0 } else { 0.0 };
                let halo = (1.0 - dist / glow).max(0.0).powi(2) * 0.7;
                let a = f32::max(core, halo) * (1.0 - p3);
                put(d, a, a, a, a);
            }
        }
    });
}

// ─── azulejos ───────────────────────────────────────────────────────────────

/// La pantalla en azulejos que se voltean de lado en ola desde el cursor;
/// la orilla del azulejo que gira, del color del look.
fn azulejos(canvas: &mut [u8], s: &Scene) {
    let tile = ((90.0 * s.sf) as usize).max(8);
    let k = 1.0 - s.dim;
    let nx = s.w.div_ceil(tile);
    par_rows(canvas, s.w, |y, row| {
        let j = y / tile;
        for i in 0..nx {
            let (x0, x1) = (i * tile, ((i + 1) * tile).min(s.w));
            let (tcx, tcy) = ((x0 as f32 + tile as f32 / 2.0), (j * tile) as f32 + tile as f32 / 2.0);
            let delay = 0.55 * ((tcx - s.cx).hypot(tcy - s.cy) / s.far).min(1.0) + 0.08 * rand01(hash((j * 4099 + i) as u32) ^ s.seed);
            let p = ease_in_out(((s.t - delay) / 0.4).clamp(0.0, 1.0));
            let sx = (p * std::f32::consts::FRAC_PI_2).cos();
            let half = tile as f32 / 2.0 * sx;
            let shade = 0.55 + 0.45 * sx;
            for x in x0..x1 {
                let d = &mut row[x * 4..x * 4 + 4];
                let u = x as f32 + 0.5 - tcx;
                if sx < 0.01 || u.abs() > half {
                    d.copy_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                let o = old(at(s, tcx + u / sx, y as f32), k * shade);
                let o = if p > 0.0 && half - u.abs() < 2.5 * s.sf { mix(o, s.accent, 0.85) } else { o };
                put(d, o[0], o[1], o[2], 1.0);
            }
        }
    });
}

// ─── cartas ─────────────────────────────────────────────────────────────────

/// Cartas de lotería: la pantalla en cartas que se voltean (sale el reverso,
/// del color del look) y luego se van, en el orden en que las cantan.
fn cartas(canvas: &mut [u8], s: &Scene) {
    let (cw, ch) = (((150.0 * s.sf) as usize).max(8), ((225.0 * s.sf) as usize).max(12));
    let k = 1.0 - s.dim;
    let nx = s.w.div_ceil(cw);
    let cream = [0.96, 0.92, 0.84];
    let back = mix(s.accent, [0.0, 0.0, 0.0], 0.25);
    par_rows(canvas, s.w, |y, row| {
        let j = y / ch;
        for i in 0..nx {
            let (x0, x1) = (i * cw, ((i + 1) * cw).min(s.w));
            let (ccx, ccy) = (x0 as f32 + cw as f32 / 2.0, (j * ch) as f32 + ch as f32 / 2.0);
            // al azar, como las cantan, con un poco de cercanía al cursor
            let delay = 0.45 * rand01(hash((j * 3001 + i) as u32) ^ s.seed) + 0.15 * ((ccx - s.cx).hypot(ccy - s.cy) / s.far).min(1.0);
            let p = ((s.t - delay) / 0.4).clamp(0.0, 1.0);
            let fy = y as f32 + 0.5 - ccy;
            for x in x0..x1 {
                let d = &mut row[x * 4..x * 4 + 4];
                let fx = x as f32 + 0.5 - ccx;
                if p < 0.4 {
                    // el frente (la foto) gira hasta quedar de canto
                    let sx = (p / 0.4 * std::f32::consts::FRAC_PI_2).cos();
                    if sx < 0.01 || fx.abs() > cw as f32 / 2.0 * sx {
                        d.copy_from_slice(&[0, 0, 0, 0]);
                        continue;
                    }
                    let o = old(at(s, ccx + fx / sx, y as f32), k * (0.6 + 0.4 * sx));
                    put(d, o[0], o[1], o[2], 1.0);
                    continue;
                }
                // el reverso: sale girando y luego se encoge y se va
                let (sx, sc) = if p < 0.75 {
                    (((p - 0.4) / 0.35 * std::f32::consts::FRAC_PI_2).sin(), 1.0)
                } else {
                    (1.0, 1.0 - ease_in_out((p - 0.75) / 0.25))
                };
                let (hw, hh) = (cw as f32 / 2.0 * sx * sc, ch as f32 / 2.0 * sc);
                if sx * sc < 0.01 || fx.abs() > hw || fy.abs() > hh {
                    d.copy_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                // en coordenadas de la carta sin escalar, para dibujar el reverso
                let (u, v) = (fx / (sx * sc), fy / sc);
                let edge = (cw as f32 / 2.0 - u.abs()).min(ch as f32 / 2.0 - v.abs()) / s.sf;
                let ring = (u.hypot(v) - cw as f32 * 0.22 / 1.0).abs() / s.sf;
                let c = if edge < 3.0 {
                    [0.1, 0.1, 0.1]
                } else if (7.0..11.0).contains(&edge) || ring < 2.0 {
                    cream
                } else {
                    back
                };
                let c = mix(c, [0.0, 0.0, 0.0], 0.35 * (1.0 - sx));
                put(d, c[0], c[1], c[2], 1.0);
            }
        }
    });
}

// ─── onda ───────────────────────────────────────────────────────────────────

/// Una ola sale del cursor: deforma la foto como agua y detrás de ella ya
/// está lo nuevo.
fn onda(canvas: &mut [u8], s: &Scene) {
    let sf = s.sf;
    let (fade, lam, decay) = (70.0 * sf, 75.0 * sf, 170.0 * sf);
    let r = (1.0 - (1.0 - s.t).powi(2)) * (s.far + fade + 40.0 * sf);
    let amp = 20.0 * sf * (1.0 - 0.5 * s.t);
    let k = 1.0 - s.dim;
    let ki = (k * 256.0) as u32;
    let tau = std::f32::consts::TAU;
    par_rows(canvas, s.w, |y, row| {
        let dy = y as f32 + 0.5 - s.cy;
        for (x, d) in row.chunks_exact_mut(4).enumerate() {
            let dx = x as f32 + 0.5 - s.cx;
            let dist = dx.hypot(dy);
            let q = dist - r;
            if q < -fade {
                d.copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            if q > 5.0 * decay {
                dim_px(d, at(s, x as f32, y as f32), ki);
                continue;
            }
            let env = if q >= 0.0 { (-q / decay).exp() } else { 1.0 };
            let ph = q / lam * tau;
            let disp = amp * env * ph.sin();
            let (ux, uy) = if dist > 0.5 { (dx / dist, dy / dist) } else { (0.0, 0.0) };
            let o = old(at(s, x as f32 - ux * disp, y as f32 - uy * disp), k);
            let shine = (ph.cos() * env * 0.22).max(0.0);
            let a = if q < 0.0 { 1.0 + q / fade } else { 1.0 };
            put(d, (o[0] + shine) * a, (o[1] + shine) * a, (o[2] + shine) * a, a);
        }
    });
}

// ─── tinta ──────────────────────────────────────────────────────────────────

/// Cuándo llega la tinta a cada pixel: manchas que nacen en el cursor y en
/// puntos al azar, crecen a distinta velocidad y se juntan, con orilla de
/// ruido.
fn ink_field(s: &Scene) -> Vec<u16> {
    let n = noise_field(s, 120.0);
    let mut seeds = vec![(s.cx, s.cy, 0.0f32, 1.0f32)];
    for i in 0..6u32 {
        let r = |k: u32| rand01(hash(i * 5 + k) ^ s.seed);
        seeds.push((r(0) * s.w as f32, r(1) * s.h as f32, 0.12 + 0.35 * r(2), 0.6 + 0.4 * r(3)));
    }
    let mut t = vec![0f32; s.w * s.h];
    // por franjas de filas en paralelo, como al pintar
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    let band = s.h.div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        for (b, rows) in t.chunks_mut(band * s.w).enumerate() {
            let (seeds, n) = (&seeds, &n);
            sc.spawn(move || {
                for (j, v) in rows.iter_mut().enumerate() {
                    let i = b * band * s.w + j;
                    let (x, y) = ((i % s.w) as f32, (i / s.w) as f32);
                    let arrive = seeds
                        .iter()
                        .map(|&(sx, sy, delay, speed)| delay + (x - sx).hypot(y - sy) / (s.far * speed))
                        .fold(f32::MAX, f32::min);
                    *v = (arrive + 0.18 * (n[i] - 0.5)).max(0.0);
                }
            });
        }
    });
    let max = t.iter().cloned().fold(1e-3, f32::max);
    t.iter().map(|v| (v / max * 65535.0) as u16).collect()
}

fn tinta(canvas: &mut [u8], s: &Scene, field: &[u16]) {
    const BAND: f32 = 0.05;
    const WET: f32 = 0.05; // lo que la tinta ya mojó antes de llegar
    let th = ease_in_out(s.t) * (1.0 + BAND + 0.02);
    let k = 1.0 - s.dim;
    let ki = (k * 256.0) as u32;
    let ink = mix(s.accent, [0.02, 0.02, 0.05], 0.7);
    par_rows(canvas, s.w, |y, row| {
        for (x, d) in row.chunks_exact_mut(4).enumerate() {
            let i = y * s.w + x;
            let v = field[i] as f32 / 65535.0;
            if v < th - BAND {
                d.copy_from_slice(&[0, 0, 0, 0]);
            } else if v < th {
                let a = (1.0 - (th - v) / BAND).powf(1.3) * 0.95;
                put(d, ink[0] * a, ink[1] * a, ink[2] * a, a);
            } else if v < th + WET {
                let f = (v - th) / WET;
                let o = mix(ink, old(&s.base[i * 4..i * 4 + 4], k), 0.35 + 0.65 * f);
                put(d, o[0], o[1], o[2], 1.0);
            } else {
                dim_px(d, &s.base[i * 4..i * 4 + 4], ki);
            }
        }
    });
}

// ─── wipe ───────────────────────────────────────────────────────────────────

/// Una barrida seca, sin suavizado: arranca del lado del cursor y cruza la
/// pantalla con una raya del color del look.
fn wipe(canvas: &mut [u8], s: &Scene) {
    let (w, h) = (s.w as f32, s.h as f32);
    let (mut dx, mut dy) = (w / 2.0 - s.cx, h / 2.0 - s.cy);
    let len = dx.hypot(dy);
    if len < 0.15 * w.min(h) {
        (dx, dy) = (0.866, 0.5);
    } else {
        (dx, dy) = (dx / len, dy / len);
    }
    let proj = |x: f32, y: f32| x * dx + y * dy;
    let corners = [proj(0.0, 0.0), proj(w, 0.0), proj(0.0, h), proj(w, h)];
    let lo = corners.iter().cloned().fold(f32::MAX, f32::min);
    let hi = corners.iter().cloned().fold(f32::MIN, f32::max);
    let bar = 10.0 * s.sf;
    let front = lo - bar + ease_in_out(s.t) * (hi - lo + 2.0 * bar);
    let ki = ((1.0 - s.dim) * 256.0) as u32;
    let a = s.accent;
    par_rows(canvas, s.w, |y, row| {
        for (x, d) in row.chunks_exact_mut(4).enumerate() {
            let p = proj(x as f32 + 0.5, y as f32 + 0.5);
            if p < front {
                d.copy_from_slice(&[0, 0, 0, 0]);
            } else if p < front + bar {
                put(d, a[0], a[1], a[2], 1.0);
            } else {
                dim_px(d, at(s, x as f32, y as f32), ki);
            }
        }
    });
}

// ─── pixelado ───────────────────────────────────────────────────────────────

/// Se pixelea cada vez más grueso y los bloques se apagan al azar (el
/// recién ido destella con el color del look).
fn pixelado(canvas: &mut [u8], s: &Scene) {
    let p = (s.t / 0.6).min(1.0);
    let bs = ((2f32.powf(p * 6.0)).round() * s.sf).max(1.0) as usize;
    let grid = ((64.0 * s.sf) as usize).max(1);
    let gone = (s.t - 0.3) / 0.62;
    let ki = ((1.0 - s.dim) * 256.0) as u32;
    let a = s.accent;
    par_rows(canvas, s.w, |y, row| {
        let by = (y / bs * bs + bs / 2).min(s.h - 1);
        for (x, d) in row.chunks_exact_mut(4).enumerate() {
            let r = rand01(hash(((y / grid) * 8191 + x / grid) as u32) ^ s.seed);
            if r < gone {
                if r > gone - 0.05 {
                    put(d, a[0] * 0.9, a[1] * 0.9, a[2] * 0.9, 0.9);
                } else {
                    d.copy_from_slice(&[0, 0, 0, 0]);
                }
                continue;
            }
            let bx = (x / bs * bs + bs / 2).min(s.w - 1);
            dim_px(d, &s.base[(by * s.w + bx) * 4..][..4], ki);
        }
    });
}

// ─── humo ───────────────────────────────────────────────────────────────────

/// La pantalla vieja se hace humo: se deshace de abajo hacia arriba con
/// orillas de ruido; lo sólido se queda quieto y solo lo que ya es humo sube y
/// se enrosca, pálido y cada vez más delgado (con un dejo del acento). Lo caro
/// (los dos ruidos) se calcula una vez.
pub struct Humo {
    /// cuándo se va cada pixel (0..1): ruido grande más la altura
    when: Vec<u16>,
    /// hacia dónde se enrosca cada pixel (-127..127): ruido suave
    swirl: Vec<i8>,
}

impl Humo {
    fn new(s: &Scene) -> Humo {
        let n = noise_field(s, 200.0);
        let m = noise_field(&Scene { seed: s.seed ^ 0x9e37_79b9, ..*s }, 160.0);
        let mut when = vec![0u16; s.w * s.h];
        let mut swirl = vec![0i8; s.w * s.h];
        for y in 0..s.h {
            // abajo se va primero, como el humo que se aleja de la brasa
            let up = 1.0 - y as f32 / s.h.max(1) as f32;
            for x in 0..s.w {
                let i = y * s.w + x;
                when[i] = ((0.6 * n[i] + 0.4 * up).clamp(0.0, 1.0) * 65535.0) as u16;
                swirl[i] = ((m[i] - 0.5) * 2.0 * 127.0).clamp(-127.0, 127.0) as i8;
            }
        }
        Humo { when, swirl }
    }

    fn paint(&self, canvas: &mut [u8], s: &Scene) {
        const BAND: f32 = 0.34; // lo que tarda un pixel en hacerse humo
        let th = smooth(s.t) * (1.0 + BAND);
        // cuánto sube y se enrosca lo que ya es humo (pixeles del buffer)
        let rise = 120.0 * s.sf;
        let curl = 45.0 * s.sf;
        let k = 1.0 - s.dim;
        let ki = (k * 256.0) as u32;
        let tint = mix([0.86, 0.86, 0.9], s.accent, 0.15);
        let (w, h) = (s.w as i32, s.h as i32);
        // qué tanto es humo un pixel (0 sólido, 1 ya se fue)
        let gone = |v: f32| smooth(1.0 - (v - (th - BAND)) / BAND);
        par_rows(canvas, s.w, |y, row| {
            for (x, d) in row.chunks_exact_mut(4).enumerate() {
                let i = y * s.w + x;
                let q = gone(self.when[i] as f32 / 65535.0);
                if q >= 1.0 {
                    d.copy_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                // todavía sólido: la foto quieta (lo más común al empezar)
                if q <= 0.0 {
                    dim_px(d, &s.base[i * 4..i * 4 + 4], ki);
                    continue;
                }
                // lo que se ve aquí viene de más abajo y de un lado: sube
                let c = self.swirl[i] as f32 / 127.0;
                let sx = (x as f32 + c * curl * q).round() as i32;
                let sy = (y as f32 + rise * q * (0.75 + 0.25 * c.abs())).round() as i32;
                if sy >= h {
                    d.copy_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                let j = (sy.max(0) * w + sx.clamp(0, w - 1)) as usize;
                let g = gone(self.when[j] as f32 / 65535.0).max(q);
                let o = old(&s.base[j * 4..j * 4 + 4], k);
                // al irse, del color de la foto a un humo pálido y delgado
                let col = mix(o, tint, g.sqrt() * 0.85);
                let rest = 1.0 - g;
                let al = rest * rest.sqrt();
                put(d, col[0] * al, col[1] * al, col[2] * al, al);
            }
        });
    }
}

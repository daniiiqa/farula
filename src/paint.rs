//! Los pixeles del telón: la foto de la pantalla vieja con un hoyo redondo
//! que crece desde el cursor (por el hoyo se ve la pantalla nueva) y un
//! anillo de luz en la orilla. Todo en BGRA premultiplicado (ARGB8888 de
//! Wayland en little endian), fila por fila: lo de fuera del anillo se copia
//! tal cual, lo de dentro se borra, y solo la franja del anillo se calcula
//! pixel a pixel.

/// Un cuadro de la animación, en pixeles del buffer.
pub struct Frame {
    pub cx: f32,
    pub cy: f32,
    /// radio del hoyo; <= 0 sin hoyo
    pub r: f32,
    /// orilla suave del hoyo
    pub edge: f32,
    /// ancho del resplandor del anillo, a cada lado
    pub glow: f32,
    /// cuánto se oscurece la foto (0 nada)
    pub dim: f32,
    /// color del anillo (0..1) y su intensidad
    pub ring: [f32; 3],
    pub ring_a: f32,
}

/// La foto (BGRA, opaca) al tamaño del buffer: vecino más cercano si el
/// buffer no mide lo mismo que la captura (escalas fraccionarias).
pub fn fit(rgb: &[u8], sw: usize, sh: usize, w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let sy = (y * sh / h.max(1)).min(sh.saturating_sub(1));
        for x in 0..w {
            let sx = (x * sw / w.max(1)).min(sw.saturating_sub(1));
            let s = (sy * sw + sx) * 3;
            let d = (y * w + x) * 4;
            out[d] = rgb[s + 2];
            out[d + 1] = rgb[s + 1];
            out[d + 2] = rgb[s];
            out[d + 3] = 255;
        }
    }
    out
}

/// Un PPM binario (P6, lo que saca `grim -t ppm`): (ancho, alto, RGB).
pub fn read_ppm(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        if data.get(i) == Some(&b'#') {
            while i < data.len() && data[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while i < data.len() && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        fields.push(std::str::from_utf8(&data[start..i]).ok()?);
    }
    if fields[0] != "P6" || fields[3] != "255" {
        return None;
    }
    let (w, h): (usize, usize) = (fields[1].parse().ok()?, fields[2].parse().ok()?);
    let px = data.get(i + 1..i + 1 + w * h * 3)?;
    Some((w, h, px.to_vec()))
}

fn old_row(dst: &mut [u8], src: &[u8], k: u32) {
    if k >= 256 {
        dst.copy_from_slice(src);
        return;
    }
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        d[0] = ((s[0] as u32 * k) >> 8) as u8;
        d[1] = ((s[1] as u32 * k) >> 8) as u8;
        d[2] = ((s[2] as u32 * k) >> 8) as u8;
        d[3] = 255;
    }
}

/// Desde dónde hasta dónde (x) cae un círculo de radio `r` en la fila a
/// distancia `dy` del centro.
fn span(cx: f32, dy: f32, r: f32, w: usize) -> Option<(usize, usize)> {
    if r <= dy.abs() {
        return None;
    }
    let hw = (r * r - dy * dy).sqrt();
    let a = (cx - hw).floor().clamp(0.0, w as f32) as usize;
    let b = (cx + hw).ceil().clamp(0.0, w as f32) as usize;
    (a < b).then_some((a, b))
}

pub fn paint(canvas: &mut [u8], base: &[u8], w: usize, h: usize, f: &Frame) {
    let k = ((1.0 - f.dim) * 256.0) as u32;
    let kf = 1.0 - f.dim;
    let inner = f.r - f.glow;
    let outer = f.r + f.edge + f.glow;
    let peak = f.r + f.edge * 0.5;
    for y in 0..h {
        let row = y * w * 4..(y + 1) * w * 4;
        let (dst, src) = (&mut canvas[row.clone()], &base[row]);
        let dy = y as f32 + 0.5 - f.cy;
        let Some((o0, o1)) = (f.r > 0.0).then(|| span(f.cx, dy, outer, w)).flatten() else {
            old_row(dst, src, k);
            continue;
        };
        old_row(&mut dst[..o0 * 4], &src[..o0 * 4], k);
        old_row(&mut dst[o1 * 4..], &src[o1 * 4..], k);
        let (i0, i1) = span(f.cx, dy, inner, w).map_or((o1, o1), |(a, b)| (a.max(o0), b.min(o1)));
        dst[i0 * 4..i1 * 4].fill(0);
        for x in (o0..i0).chain(i1..o1) {
            let dx = x as f32 + 0.5 - f.cx;
            let d = (dx * dx + dy * dy).sqrt();
            // lo viejo: nada dentro del hoyo, completo pasando la orilla
            let a_old = ((d - f.r) / f.edge).clamp(0.0, 1.0);
            // el anillo: sube desde dentro del hoyo, pico en media orilla,
            // y se apaga hacia fuera; al cuadrado para que parezca luz
            let t = if d < peak { (d - inner) / (peak - inner) } else { 1.0 - (d - peak) / (outer - peak) };
            let t = t.clamp(0.0, 1.0);
            let ra = f.ring_a * t * t;
            // un hilo blanco justo en el pico
            let core = (1.0 - (d - peak).abs() / 2.5).clamp(0.0, 1.0) * 0.6 * f.ring_a;
            let s = &src[x * 4..x * 4 + 4];
            let mut px = [0f32; 4];
            for c in 0..3 {
                let old = s[c] as f32 / 255.0 * kf * a_old;
                // ring en BGRA: el canal 0 es azul
                let ring = f.ring[2 - c] * ra;
                let v = ring + old * (1.0 - ra);
                px[c] = core + v * (1.0 - core);
            }
            let a = ra + a_old * (1.0 - ra);
            px[3] = core + a * (1.0 - core);
            let d = &mut dst[x * 4..x * 4 + 4];
            for c in 0..4 {
                d[c] = (px[c] * 255.0 + 0.5) as u8;
            }
        }
    }
}

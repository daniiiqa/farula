//! farula: el telón de discada. Congela la pantalla (una foto por monitor en
//! un layer de overlay), deja que el cambio de wallpaper o de look pase por
//! debajo y luego lo revela con un efecto (fx.rs): iris, quemado, glitch,
//! vidrio, derretido o fade, en el color nuevo. Así wallpaper, barra, bordes
//! y kitty cambian juntos, en un solo golpe.
//!
//!   farula freeze        congela; vuelve cuando el telón ya está en pantalla.
//!                        Sale con 1 si está apagado o no pudo: el script usa
//!                        su transición de awww de siempre
//!     --image F.ppm      en vez de una foto de la pantalla, esta imagen (el
//!                        lock: bloqueado no se puede capturar el escritorio)
//!     --hold             sin el tope de 15 s (el lock dura lo que dure)
//!   farula reveal [fx]   lo abre; sin fx, el del look ([farula] fx en su
//!                        look.toml: un nombre, una lista o "random"); iris si
//!                        el look no dice
//!   farula demo [fx]     congela y abre, para verlo
//!   farula list          los efectos
//!   farula active        sale con 0 si hay telón puesto
//!
//! Apagarlo: touch ~/.config/hypr/farula.off
//! El telón no agarra clics (región de input vacía) y si nadie lo revela se
//! abre solo a los 15 s.

mod fx;
mod paint;

use fx::{Data, Fx, Scene};
use paint::Frame;
use serde_json::Value;
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop::channel::{Event, channel};
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{wl_output, wl_shm, wl_surface};
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// la foto se oscurece un poco: "algo está pasando"
const INTRO: f32 = 0.28;
const DIM: f32 = 0.22;
const SAFETY: Duration = Duration::from_secs(15);

fn runtime() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from)
}
fn pidfile() -> PathBuf {
    runtime().join("farula.pid")
}
fn readyfile() -> PathBuf {
    runtime().join("farula.ready")
}
/// el efecto que pidió `farula reveal <fx>` (la señal no lleva datos)
fn fxfile() -> PathBuf {
    runtime().join("farula.fx")
}
fn home(p: &str) -> PathBuf {
    std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from).join(p)
}

fn running() -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(pidfile()).ok()?.trim().parse().ok()?;
    (unsafe { libc::kill(pid, 0) } == 0).then_some(pid)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str);
    let fx_arg = matches!(cmd, Some("reveal" | "demo")).then(|| args.get(2).map(String::as_str)).flatten();
    if let Some(name) = fx_arg
        && name != "random"
        && Fx::parse(name).is_none()
    {
        eprintln!("farula: no conozco el efecto '{name}' (farula list)");
        std::process::exit(2);
    }
    let code = match cmd {
        Some("freeze") => freeze(&args[2..]),
        Some("reveal") => reveal(fx_arg),
        Some("demo") => {
            if freeze(&[]) != 0 {
                eprintln!("farula: no se pudo congelar (¿farula.off?)");
                1
            } else {
                std::thread::sleep(Duration::from_millis(600));
                reveal(fx_arg)
            }
        }
        Some("list") => {
            for f in fx::ALL {
                println!("{}", f.name());
            }
            println!("random");
            0
        }
        Some("active") => i32::from(running().is_none()),
        Some("_run") => match run(Opts::parse(&args[2..])) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("farula: {e}");
                1
            }
        },
        _ => {
            eprintln!("uso: farula freeze | reveal [fx] | demo [fx] | list | active");
            2
        }
    };
    std::process::exit(code);
}

fn reveal(fx: Option<&str>) -> i32 {
    let Some(pid) = running() else { return 0 };
    match fx {
        Some(name) => {
            let _ = std::fs::write(fxfile(), name);
        }
        None => {
            let _ = std::fs::remove_file(fxfile());
        }
    }
    unsafe { libc::kill(pid, libc::SIGUSR1) };
    0
}

/// El efecto del look actual: `[farula]` → `fx = "quemado"`, una lista
/// (`fx = ["quemado", "vidrio"]`, uno al azar) o "random". Se lee al revelar,
/// así que al cambiar de look sale el del look nuevo.
fn look_fx() -> Vec<String> {
    let looks = home(".config/hypr/looks");
    let Ok(look) = std::fs::read_to_string(looks.join(".current")) else { return Vec::new() };
    let Ok(toml) = std::fs::read_to_string(looks.join(look.trim()).join("look.toml")) else { return Vec::new() };
    let mut inside = false;
    for line in toml.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            inside = line == "[farula]";
        } else if inside && let Some((k, v)) = line.split_once('=')
            && k.trim() == "fx"
        {
            return v.trim().trim_matches(['[', ']']).split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect();
        }
    }
    Vec::new()
}

/// Uno de `names` al azar ("random": cualquiera); iris si no hay.
fn pick(names: &[String], seed: u32) -> Fx {
    let mut options: Vec<Fx> = Vec::new();
    for n in names {
        if n == "random" {
            options.extend(fx::ALL.into_iter().filter(|f| *f != Fx::Fade));
        } else if let Some(f) = Fx::parse(n) {
            options.push(f);
        }
    }
    if options.is_empty() {
        return Fx::Iris;
    }
    options[seed as usize % options.len()]
}

/// Lanza el telón en su propia sesión y espera a que esté pintado: si
/// volviera antes, el cambio se vería antes de que lo tape.
fn freeze(flags: &[String]) -> i32 {
    if home(".config/hypr/farula.off").exists() {
        return 1;
    }
    if running().is_some() {
        return 0;
    }
    let _ = std::fs::remove_file(readyfile());
    let Ok(exe) = std::env::current_exe() else { return 1 };
    let mut cmd = Command::new(exe);
    cmd.arg("_run").args(flags).stdin(Stdio::null()).stdout(Stdio::null());
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let Ok(mut child) = cmd.spawn() else { return 1 };
    for _ in 0..80 {
        if readyfile().exists() {
            return 0;
        }
        if let Ok(Some(_)) = child.try_wait() {
            return 1;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    1
}

fn hypr(what: &str) -> Value {
    Command::new("hyprctl")
        .args([what, "-j"])
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or(Value::Null)
}

/// Un monitor según Hyprland: dónde está (lógico) y su foto.
struct Mon {
    name: String,
    x: f32,
    y: f32,
    shot: Option<(usize, usize, Vec<u8>)>,
}

/// El color de acento del look recién puesto (lo lee al revelar).
fn accent() -> [f32; 3] {
    let colors: Value = std::fs::read(home(".config/empalme/colors.json"))
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or(Value::Null);
    let hex = ["acc", "color12", "foreground"].iter().find_map(|k| colors[k].as_str()).unwrap_or("#ffffff");
    let hex = hex.trim_start_matches('#');
    let ch = |i: usize| hex.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).unwrap_or(255) as f32 / 255.0;
    [ch(0), ch(2), ch(4)]
}

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Intro,
    Hold,
    Reveal,
}

struct Out {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    mon: usize,
    size: (u32, u32),
    scale: i32,
    pool: Option<SlotPool>,
    base: Vec<u8>,
    /// lo que el efecto precalcula para este monitor (al revelar)
    data: Option<Data>,
    frames: u32,
    frame_pending: bool,
    done: bool,
}

struct App {
    registry: RegistryState,
    output: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    qh: QueueHandle<App>,
    mons: Vec<Mon>,
    outs: Vec<Out>,
    cursor: (f32, f32),
    /// hasta la esquina más lejana de todos los monitores
    far: f32,
    phase: Phase,
    t0: Instant,
    accent: [f32; 3],
    fx: Fx,
    seed: u32,
    /// cuánto se oscurece la foto (con --image nada: debe verse igual que el
    /// lock que se va)
    dim: f32,
    /// FARULA_DEBUG=1: cuánto tarda precalcular y cada cuadro (farula.log)
    prep_ms: f32,
    paint_ms: Vec<f32>,
    ready: bool,
    exit: bool,
}

impl App {
    fn add_out(&mut self, output: wl_output::WlOutput) {
        if self.outs.iter().any(|o| o.output == output) {
            return;
        }
        let name = self.output.info(&output).and_then(|i| i.name).unwrap_or_default();
        let Some(mon) = self.mons.iter().position(|m| m.name == name && m.shot.is_some()) else { return };
        let surface = self.compositor.create_surface(&self.qh);
        if let Ok(region) = Region::new(&self.compositor) {
            surface.set_input_region(Some(region.wl_region()));
        }
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("farula"), Some(&output));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        layer.commit();
        self.outs.push(Out {
            output,
            layer,
            mon,
            size: (0, 0),
            scale: 1,
            pool: None,
            base: Vec::new(),
            data: None,
            frames: 0,
            frame_pending: false,
            done: false,
        });
    }

    fn animating(&self) -> bool {
        self.phase != Phase::Hold
    }

    /// La escena del monitor `o` (pixeles del buffer).
    fn scene<'a>(&self, o: &'a Out, t: f32, dim: f32) -> Scene<'a> {
        let s = o.scale.max(1) as f32;
        let m = &self.mons[o.mon];
        Scene {
            base: &o.base,
            w: o.size.0 as usize * s as usize,
            h: o.size.1 as usize * s as usize,
            cx: (self.cursor.0 - m.x) * s,
            cy: (self.cursor.1 - m.y) * s,
            far: self.far * s,
            sf: s,
            t,
            dim,
            accent: self.accent,
            seed: self.seed,
        }
    }

    fn reveal(&mut self) {
        if self.phase == Phase::Reveal {
            return;
        }
        self.accent = accent();
        self.seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.subsec_nanos());
        // `farula reveal <fx>` gana; si no, el del look (ya el nuevo)
        let asked = std::fs::read_to_string(fxfile()).ok();
        let _ = std::fs::remove_file(fxfile());
        let names = asked.map(|a| vec![a.trim().to_string()]).unwrap_or_else(look_fx);
        self.fx = pick(&names, fx::hash(self.seed));
        // lo caro antes de arrancar el reloj, o el primer cuadro brincaría
        let prep = Instant::now();
        for i in 0..self.outs.len() {
            let o = &self.outs[i];
            if o.base.is_empty() {
                continue;
            }
            let data = fx::prepare(self.fx, &self.scene(o, 0.0, DIM));
            self.outs[i].data = Some(data);
        }
        self.prep_ms = prep.elapsed().as_secs_f32() * 1000.0;
        self.phase = Phase::Reveal;
        self.t0 = Instant::now();
        // de una vez, aunque haya un aviso de cuadro pendiente: bajo el lock
        // (--hold) los avisos no llegan y se quedaba esperando uno viejo
        for i in 0..self.outs.len() {
            self.outs[i].frame_pending = false;
            self.draw(i);
        }
    }

    fn draw(&mut self, i: usize) {
        let now = self.t0.elapsed().as_secs_f32();
        if self.phase == Phase::Intro && now >= INTRO {
            self.phase = Phase::Hold;
        }
        let t = if self.phase == Phase::Reveal { now / self.fx.secs() } else { 0.0 };
        let dim = match self.phase {
            Phase::Intro => self.dim * now / INTRO,
            Phase::Hold => self.dim,
            // la foto se aclara mientras se va
            Phase::Reveal => self.dim * (1.0 - t.min(1.0)),
        };
        let finished = t >= 1.0;
        let animating = self.animating();

        let s = self.outs[i].scale.max(1);
        let (w, h) = (self.outs[i].size.0 as i32 * s, self.outs[i].size.1 as i32 * s);
        if w <= 0 || h <= 0 {
            return;
        }
        let (wu, hu) = (w as usize, h as usize);
        if self.outs[i].base.len() != wu * hu * 4 {
            let (sw, sh, rgb) = self.mons[self.outs[i].mon].shot.as_ref().expect("monitor sin foto");
            let base = paint::fit(rgb, *sw, *sh, wu, hu);
            let o = &mut self.outs[i];
            o.base = base;
            o.pool = None;
            o.data = None;
        }
        if self.phase == Phase::Reveal && self.outs[i].data.is_none() {
            let data = fx::prepare(self.fx, &self.scene(&self.outs[i], 0.0, dim));
            self.outs[i].data = Some(data);
        }
        let mut pool = match self.outs[i].pool.take() {
            Some(p) => p,
            None => SlotPool::new(wu * hu * 4, &self.shm).expect("memoria compartida"),
        };
        let Ok((buffer, canvas)) = pool.create_buffer(w, h, w * 4, wl_shm::Format::Argb8888) else {
            self.outs[i].pool = Some(pool);
            return;
        };
        let o = &self.outs[i];
        let scene = self.scene(o, t, dim);
        match (&self.phase, &o.data) {
            (Phase::Reveal, Some(data)) => {
                let p = Instant::now();
                fx::paint(self.fx, data, canvas, &scene);
                self.paint_ms.push(p.elapsed().as_secs_f32() * 1000.0);
            }
            // antes de revelar: la foto, oscureciéndose
            _ => paint::paint(canvas, &o.base, wu, hu, &Frame {
                cx: scene.cx,
                cy: scene.cy,
                r: 0.0,
                edge: 1.0,
                glow: 1.0,
                dim,
                ring: self.accent,
                ring_a: 0.0,
            }),
        }

        let o = &mut self.outs[i];
        o.pool = Some(pool);
        let surface = o.layer.wl_surface();
        surface.set_buffer_scale(s);
        surface.damage_buffer(0, 0, w, h);
        if animating && !finished {
            surface.frame(&self.qh, FrameCallbackData(surface.clone()));
            o.frame_pending = true;
        }
        if buffer.attach_to(surface).is_ok() {
            o.layer.commit();
        }
        o.frames += 1;
        o.done = finished;
        if self.phase == Phase::Reveal && self.outs.iter().all(|o| o.done) {
            self.exit = true;
        }
    }
}

/// Lo que `freeze` le pasa al telón.
struct Opts {
    image: Option<PathBuf>,
    hold: bool,
}

impl Opts {
    fn parse(args: &[String]) -> Opts {
        let mut o = Opts { image: None, hold: false };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--image" => o.image = it.next().map(PathBuf::from),
                "--hold" => o.hold = true,
                _ => {}
            }
        }
        o
    }
}

fn run(opts: Opts) -> Result<(), String> {
    // SIGUSR1 (reveal) se espera en un hilo: bloquearla antes de abrirlo
    let sigs = unsafe {
        let mut s: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut s);
        libc::sigaddset(&mut s, libc::SIGUSR1);
        libc::pthread_sigmask(libc::SIG_BLOCK, &s, std::ptr::null_mut());
        s
    };
    std::fs::write(pidfile(), std::process::id().to_string()).map_err(|e| e.to_string())?;
    let result = curtain(sigs, opts);
    let _ = std::fs::remove_file(pidfile());
    let _ = std::fs::remove_file(readyfile());
    result
}

fn curtain(sigs: libc::sigset_t, opts: Opts) -> Result<(), String> {
    // las fotos, antes de poner nada encima
    let dir = runtime().join("farula");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut mons: Vec<Mon> = hypr("monitors")
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| !m["disabled"].as_bool().unwrap_or(false))
        .map(|m| Mon {
            name: m["name"].as_str().unwrap_or_default().to_string(),
            x: m["x"].as_f64().unwrap_or(0.0) as f32,
            y: m["y"].as_f64().unwrap_or(0.0) as f32,
            shot: None,
        })
        .collect();
    // con --image, la misma imagen en todos los monitores (ya viene a su
    // tamaño; si no, paint::fit la estira)
    let image = opts.image.as_ref().map(|p| std::fs::read(p).ok().and_then(|d| paint::read_ppm(&d)).ok_or("la imagen de --image no es un PPM"));
    let image = image.transpose()?;
    let grabs: Vec<_> = mons
        .iter()
        .filter(|_| image.is_none())
        .map(|m| {
            let path = dir.join(format!("{}.ppm", m.name));
            let child = Command::new("grim").args(["-t", "ppm", "-o", &m.name]).arg(&path).spawn();
            (path, child)
        })
        .collect();
    if let Some(img) = &image {
        for m in &mut mons {
            m.shot = Some(img.clone());
        }
    }
    for (m, (path, child)) in mons.iter_mut().zip(grabs) {
        if child.and_then(|mut c| c.wait()).is_ok_and(|s| s.success()) {
            m.shot = std::fs::read(&path).ok().and_then(|d| paint::read_ppm(&d));
        }
        let _ = std::fs::remove_file(path);
    }
    if mons.iter().all(|m| m.shot.is_none()) {
        return Err("grim no pudo capturar la pantalla".into());
    }
    let cur = hypr("cursorpos");
    let cursor = (cur["x"].as_f64().unwrap_or(0.0) as f32, cur["y"].as_f64().unwrap_or(0.0) as f32);
    // el tamaño lógico de cada monitor sale de su foto: grim captura en
    // pixeles físicos, ya rotados; la escala la da Hyprland
    let mut far: f32 = 1.0;
    let mon_json = hypr("monitors");
    for m in &mons {
        let Some((sw, sh, _)) = &m.shot else { continue };
        let scale = mon_json
            .as_array()
            .and_then(|a| a.iter().find(|j| j["name"] == m.name.as_str()))
            .and_then(|j| j["scale"].as_f64())
            .unwrap_or(1.0) as f32;
        let (w, h) = (*sw as f32 / scale, *sh as f32 / scale);
        for x in [m.x, m.x + w] {
            for y in [m.y, m.y + h] {
                far = far.max((x - cursor.0).hypot(y - cursor.1));
            }
        }
    }

    let conn = Connection::connect_to_env().map_err(|e| format!("Wayland: {e}"))?;
    let (globals, queue) = registry_queue_init(&conn).map_err(|e| format!("Wayland: {e}"))?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<App> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue).insert(handle.clone()).map_err(|e| e.to_string())?;

    let (tx, rx) = channel::<()>();
    std::thread::spawn(move || {
        loop {
            let mut sig = 0;
            if unsafe { libc::sigwait(&sigs, &mut sig) } == 0 && tx.send(()).is_err() {
                return;
            }
        }
    });
    handle
        .insert_source(rx, |ev, _, app: &mut App| {
            if let Event::Msg(()) = ev {
                app.reveal();
            }
        })
        .map_err(|e| e.to_string())?;
    // si nadie lo revela (el script tronó), que no se quede puesto; con
    // --hold no hay tope (el lock lo quita al desbloquear)
    if !opts.hold {
        handle
            .insert_source(Timer::from_duration(SAFETY), |_, _, app: &mut App| {
                app.reveal();
                TimeoutAction::Drop
            })
            .map_err(|e| e.to_string())?;
    }
    // listo cuando todos los monitores ya mostraron un par de cuadros: el
    // caller ya puede cambiar todo por debajo. Con --image basta uno: bajo el
    // lock no llegan los avisos de cuadro (nada se ve)
    let need = if image.is_some() { 1 } else { 3 };
    handle
        .insert_source(Timer::from_duration(Duration::from_millis(20)), move |_, _, app: &mut App| {
            if !app.outs.is_empty() && app.outs.iter().all(|o| o.frames >= need) {
                app.ready = true;
                let _ = std::fs::write(readyfile(), "");
                return TimeoutAction::Drop;
            }
            TimeoutAction::ToDuration(Duration::from_millis(20))
        })
        .map_err(|e| e.to_string())?;

    let mut app = App {
        registry: RegistryState::new(&globals),
        output: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh).map_err(|_| "el compositor no tiene wl_compositor")?,
        layer_shell: LayerShell::bind(&globals, &qh).map_err(|_| "el compositor no tiene layer-shell")?,
        shm: Shm::bind(&globals, &qh).map_err(|_| "el compositor no tiene wl_shm")?,
        qh,
        mons,
        outs: Vec::new(),
        cursor,
        far,
        phase: Phase::Intro,
        t0: Instant::now(),
        accent: [1.0; 3],
        fx: Fx::Iris,
        seed: 0,
        dim: if image.is_some() { 0.0 } else { DIM },
        prep_ms: 0.0,
        paint_ms: Vec::new(),
        ready: false,
        exit: false,
    };
    while !app.exit {
        event_loop.dispatch(Some(Duration::from_millis(100)), &mut app).map_err(|e| e.to_string())?;
    }
    let _ = conn.flush();
    if std::env::var_os("FARULA_DEBUG").is_some() && !app.paint_ms.is_empty() {
        let n = app.paint_ms.len() as f32;
        let avg = app.paint_ms.iter().sum::<f32>() / n;
        let max = app.paint_ms.iter().cloned().fold(0.0, f32::max);
        let line = format!(
            "{}: precalcular {:.0} ms, {} cuadros en {:.2} s ({:.0} fps), pintar prom {:.1} ms, máx {:.1} ms\n",
            app.fx.name(), app.prep_ms, n, app.fx.secs(), n / app.fx.secs(), avg, max
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(runtime().join("farula.log")) {
            let _ = f.write_all(line.as_bytes());
        }
    }
    Ok(())
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, factor: i32) {
        if let Some(i) = self.outs.iter().position(|o| o.layer.wl_surface() == surface)
            && factor != self.outs[i].scale
        {
            self.outs[i].scale = factor;
            self.draw(i);
        }
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        let Some(i) = self.outs.iter().position(|o| o.layer.wl_surface() == surface) else { return };
        self.outs[i].frame_pending = false;
        if self.animating() || !self.ready {
            self.draw(i);
        }
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_out(output);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        // el nombre puede llegar después del new_output
        self.add_out(output);
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.outs.retain(|o| o.output != output);
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.outs.retain(|o| &o.layer != layer);
        if self.outs.is_empty() {
            self.exit = true;
        }
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, cfg: LayerSurfaceConfigure, _: u32) {
        let Some(i) = self.outs.iter().position(|o| &o.layer == layer) else { return };
        if cfg.new_size.0 > 0 && cfg.new_size.1 > 0 {
            self.outs[i].size = cfg.new_size;
        }
        if !self.outs[i].frame_pending {
            self.draw(i);
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_dispatch2!(App);

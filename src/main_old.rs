//! cm-shim: speaks wp_color_manager_v1 on behalf of an app that doesn't.
//!
//!   cm-shim [-s SPACE] [-i INTENT] run <app> [args...]
//!   cm-shim [-s SPACE] [-i INTENT] install <app>
//!   cm-shim uninstall <app>
//!
//! The shim contains NO colour math and NO colorimetric values.
//!   bypass : hands the compositor's own image-description object back to it.
//!   declare: passes protocol enum names (primaries + transfer function) through.
//!
//! NOTE: written against the wl-proxy 0.1.4 docs without a compiler at hand.
//! Spots that are educated guesses at the generated API are marked `GUESS:`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use wl_proxy::baseline::Baseline;
use wl_proxy::global_mapper::GlobalMapper;
use wl_proxy::object::{Object, ObjectCoreApi};
use wl_proxy::protocols::ObjectInterface;
use wl_proxy::protocols::color_management_v1::wp_color_management_surface_feedback_v1::*;
use wl_proxy::protocols::color_management_v1::wp_color_management_surface_v1::*;
use wl_proxy::protocols::color_management_v1::wp_color_manager_v1::*;
use wl_proxy::protocols::color_management_v1::wp_image_description_v1::*;
use wl_proxy::protocols::wayland::wl_compositor::{WlCompositor, WlCompositorHandler};
use wl_proxy::protocols::wayland::wl_display::{WlDisplay, WlDisplayHandler};
use wl_proxy::protocols::wayland::wl_registry::{WlRegistry, WlRegistryHandler};
use wl_proxy::protocols::wayland::wl_surface::{WlSurface, WlSurfaceHandler};
use wl_proxy::simple::{SimpleCommandExt, SimpleProxy};

// ---------------------------------------------------------------- config ---

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Bypass,
    Declare,
}

#[derive(Clone, Copy)]
struct Config {
    mode: Mode,
    primaries: u32, // protocol enum value, declare mode only
    tf: u32,        // protocol enum value, declare mode only
    intent: u32,    // protocol enum value
}

/// `space` names follow darktable's bundled profiles. Each maps to a pair of
/// protocol enum values (named primaries, named transfer function) and to
/// nothing else: the shim holds no colorimetric numbers of its own.
///
/// Deliberately absent, because the protocol has no exact named equivalent:
///   sRGB            - what the compositor assumes anyway; don't use the shim
///   Rec709 RGB      - darktable uses the Rec.709 camera curve, not BT.1886
///   ProPhoto RGB    - no named primaries in the protocol
fn space_by_name(s: &str) -> Option<(u32, u32)> {
    const SRGB: u32 = 1; const BT2020: u32 = 6; const DISPLAY_P3: u32 = 9; const ADOBE_RGB: u32 = 10;
    const GAMMA22: u32 = 2; const EXT_LINEAR: u32 = 5; const TF_SRGB: u32 = 9; const PQ: u32 = 11; const HLG: u32 = 13;
    Some(match s {
        "adobe_rgb" => (ADOBE_RGB, GAMMA22),
        // Not a darktable bundle: needs a Rec.2020 gamma 2.2 ICC loaded in the app.
        "rec2020_g22" => (BT2020, GAMMA22),
        "display_p3" => (DISPLAY_P3, TF_SRGB),
        "linear_rec709" => (SRGB, EXT_LINEAR),
        "linear_rec2020" => (BT2020, EXT_LINEAR),
        "pq_rec2020" => (BT2020, PQ),
        "hlg_rec2020" => (BT2020, HLG),
        "pq_p3" => (DISPLAY_P3, PQ),
        "hlg_p3" => (DISPLAY_P3, HLG),
        _ => return None,
    })
}

fn intent_by_name(s: &str) -> Option<u32> {
    Some(match s {
        "perceptual" => 0, "relative" => 1, "saturation" => 2, "absolute" => 3,
        "relative_bpc" => 4,
        _ => return None,
    })
}

fn die(msg: &str) -> ! {
    eprintln!("[cm-shim] {msg}");
    std::process::exit(1);
}

/// ~/.config/cm-shim/config, `key = value` lines, `#` comments.
///   space  = display | adobe_rgb | display_p3 | linear_rec2020 | ...
///   intent = perceptual | relative | relative_bpc | absolute
/// Missing file = `space = display` (bypass), perceptual.
fn load_config() -> Config {
    let mut cfg = Config { mode: Mode::Bypass, primaries: 0, tf: 0, intent: 0 };
    let base = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
    let Ok(text) = std::fs::read_to_string(format!("{base}/cm-shim/config")) else {
        return cfg;
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap().trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        apply_setting(&mut cfg, k, v);
    }
    cfg
}

/// Shared by the config file and the command line flags.
fn apply_setting(cfg: &mut Config, key: &str, v: &str) {
    match key {
        "space" if v == "display" => cfg.mode = Mode::Bypass,
        "space" => {
            let (p, t) = space_by_name(v).unwrap_or_else(|| die(&format!("unknown space '{v}'")));
            *cfg = Config { mode: Mode::Declare, primaries: p, tf: t, ..*cfg };
        }
        "intent" => cfg.intent = intent_by_name(v)
            .unwrap_or_else(|| die(&format!("unknown intent '{v}'"))),
        _ => die(&format!("unknown setting '{key}'")),
    }
}

// --------------------------------------------------- per-connection state ---

/// One per app connection (an app may open several).
struct Ctx {
    cfg: Config,
    /// The shim's own binding of the compositor's colour manager.
    mgr: RefCell<Option<Rc<WpColorManagerV1>>>,
    /// declare mode: the one description shared by all surfaces, once ready.
    declared: RefCell<Option<Rc<WpImageDescriptionV1>>>,
    /// declare mode: surfaces created before `declared` became ready.
    waiting: RefCell<Vec<(Rc<WpColorManagementSurfaceV1>, Rc<WlSurface>)>>,
}

// GUESS: enum arguments are generated as newtype wrappers around u32.
fn intent(cfg: &Config) -> WpColorManagerV1RenderIntent {
    WpColorManagerV1RenderIntent(cfg.intent)
}

// ---------------------------------------------------------------- display ---

struct Display {
    cfg: Config,
}

impl WlDisplayHandler for Display {
    fn handle_get_registry(&mut self, slf: &Rc<WlDisplay>, registry: &Rc<WlRegistry>) {
        slf.send_get_registry(registry);
        registry.set_handler(Registry {
            filter: GlobalMapper::default(),
            ctx: Rc::new(Ctx {
                cfg: self.cfg,
                mgr: RefCell::new(None),
                declared: RefCell::new(None),
                waiting: RefCell::new(Vec::new()),
            }),
        });
    }
}

// --------------------------------------------------------------- registry ---

struct Registry {
    filter: GlobalMapper,
    ctx: Rc<Ctx>,
}

impl WlRegistryHandler for Registry {
    fn handle_global(
        &mut self,
        slf: &Rc<WlRegistry>,
        name: u32,
        interface: ObjectInterface,
        version: u32,
    ) {
        if interface != ObjectInterface::WpColorManagerV1 {
            // (frog_color_management_factory_v1 is unknown to wl-proxy and is
            // therefore filtered out automatically.)
            self.filter.forward_global(slf, name, interface, version);
            return;
        }
        // Hide it from the app; bind it for ourselves. v1 is all we need.
        self.filter.ignore_global(name);
        if self.ctx.mgr.borrow().is_some() {
            return;
        }
        let mgr = slf.state().create_object::<WpColorManagerV1>(1);
        mgr.set_forward_to_client(false);
        mgr.set_handler(Manager { ctx: self.ctx.clone(), primaries_ok: false, tf_ok: false, intent_ok: false, parametric: false });
        // GUESS: wl_registry.bind takes the untyped new object as Rc<dyn Object>.
        slf.send_bind(name, mgr.clone());
        *self.ctx.mgr.borrow_mut() = Some(mgr);
    }

    fn handle_global_remove(&mut self, slf: &Rc<WlRegistry>, name: u32) {
        self.filter.forward_global_remove(slf, name);
    }

    fn handle_bind(&mut self, slf: &Rc<WlRegistry>, name: u32, id: Rc<dyn Object>) {
        self.filter.forward_bind(slf, name, &id);
        if id.interface() == ObjectInterface::WlCompositor {
            // GUESS: downcasting Rc<dyn Object>. If this doesn't compile, look
            // for wl-proxy's own downcast helper on `Object`.
            let any: Rc<dyn std::any::Any> = id;
            if let Ok(comp) = any.downcast::<WlCompositor>() {
                comp.set_handler(Compositor { ctx: self.ctx.clone() });
            }
        }
    }
}

// ---------------------------------------------------------- colour manager ---

struct Manager {
    ctx: Rc<Ctx>,
    primaries_ok: bool,
    tf_ok: bool,
    intent_ok: bool,
    parametric: bool,
}

impl WpColorManagerV1Handler for Manager {
    fn handle_supported_intent(&mut self, _slf: &Rc<WpColorManagerV1>, v: WpColorManagerV1RenderIntent) {
        self.intent_ok |= v.0 == self.ctx.cfg.intent;
    }
    fn handle_supported_feature(&mut self, _slf: &Rc<WpColorManagerV1>, v: WpColorManagerV1Feature) {
        self.parametric |= v.0 == 1;
    }
    fn handle_supported_tf_named(&mut self, _slf: &Rc<WpColorManagerV1>, v: WpColorManagerV1TransferFunction) {
        self.tf_ok |= v.0 == self.ctx.cfg.tf;
    }
    fn handle_supported_primaries_named(&mut self, _slf: &Rc<WpColorManagerV1>, v: WpColorManagerV1Primaries) {
        self.primaries_ok |= v.0 == self.ctx.cfg.primaries;
    }

    fn handle_done(&mut self, slf: &Rc<WpColorManagerV1>) {
        let cfg = &self.ctx.cfg;
        if !self.intent_ok {
            die("compositor does not support the configured render intent");
        }
        if cfg.mode == Mode::Bypass {
            eprintln!("[cm-shim] bypass: mirroring the compositor's preferred image description");
            return;
        }
        if !self.parametric || !self.primaries_ok || !self.tf_ok {
            die("compositor does not advertise the primaries/transfer function this space needs (see `wayland-info`)");
        }
        let creator = slf.new_send_create_parametric_creator();
        creator.set_forward_to_client(false);
        creator.send_set_primaries_named(WpColorManagerV1Primaries(cfg.primaries));
        creator.send_set_tf_named(WpColorManagerV1TransferFunction(cfg.tf));
        let desc = creator.new_send_create();
        desc.set_forward_to_client(false);
        desc.set_handler(DeclaredDesc { ctx: self.ctx.clone() });
        eprintln!("[cm-shim] declare: primaries={} tf={} (protocol enum values)", cfg.primaries, cfg.tf);
    }
}

/// declare mode: the single shared description.
struct DeclaredDesc {
    ctx: Rc<Ctx>,
}

impl WpImageDescriptionV1Handler for DeclaredDesc {
    fn handle_ready(&mut self, slf: &Rc<WpImageDescriptionV1>, identity: u32) {
        eprintln!("[cm-shim] declared description ready (compositor id {identity})");
        for (cm, surface) in self.ctx.waiting.borrow_mut().drain(..) {
            cm.send_set_image_description(slf, intent(&self.ctx.cfg));
            apply_now(&surface);
        }
        *self.ctx.declared.borrow_mut() = Some(slf.clone());
    }
    fn handle_failed(&mut self, _slf: &Rc<WpImageDescriptionV1>, _cause: WpImageDescriptionV1Cause, msg: &str) {
        die(&format!("compositor rejected the declared description: {msg}"));
    }
}

// ---------------------------------------------------------------- surfaces ---

/// EXPERIMENT: an image description is pending state and only applies on the
/// next wl_surface.commit. An idle app never commits, so the shim commits for
/// it. Set CM_SHIM_NO_COMMIT=1 to get the old wait-for-the-app behaviour.
fn apply_now(surface: &Rc<WlSurface>) {
    if std::env::var_os("CM_SHIM_NO_COMMIT").is_none() {
        // Damage so the compositor repaints with the new description.
        surface.send_damage(0, 0, i32::MAX, i32::MAX);
        surface.send_commit();
    }
}

struct Compositor {
    ctx: Rc<Ctx>,
}

impl WlCompositorHandler for Compositor {
    fn handle_create_surface(&mut self, slf: &Rc<WlCompositor>, id: &Rc<WlSurface>) {
        slf.send_create_surface(id);
        let Some(mgr) = self.ctx.mgr.borrow().clone() else {
            return; // compositor has no colour management: plain passthrough
        };
        let cm = mgr.new_send_get_surface(id);
        cm.set_forward_to_client(false);
        let mut feedback = None;
        let alive = Rc::new(Cell::new(true));
        match self.ctx.cfg.mode {
            Mode::Declare => match self.ctx.declared.borrow().as_ref() {
                Some(desc) => cm.send_set_image_description(desc, intent(&self.ctx.cfg)),
                None => self.ctx.waiting.borrow_mut().push((cm.clone(), id.clone())),
            },
            Mode::Bypass => {
                let fb = mgr.new_send_get_surface_feedback(id);
                fb.set_forward_to_client(false);
                fb.set_handler(Feedback { ctx: self.ctx.clone(), cm: cm.clone(), surface: id.clone(), alive: alive.clone() });
                request_preferred(&fb, &cm, id, &alive, &self.ctx);
                feedback = Some(fb);
            }
        }
        id.set_handler(Surface { ctx: self.ctx.clone(), cm, feedback, alive });
    }
}

/// bypass mode: ask what the compositor wants for this surface; the answer
/// (its own object, untouched) goes straight back onto the surface.
fn request_preferred(
    fb: &Rc<WpColorManagementSurfaceFeedbackV1>,
    cm: &Rc<WpColorManagementSurfaceV1>,
    surface: &Rc<WlSurface>,
    alive: &Rc<Cell<bool>>,
    ctx: &Rc<Ctx>,
) {
    let desc = fb.new_send_get_preferred();
    desc.set_forward_to_client(false);
    desc.set_handler(MirrorDesc { ctx: ctx.clone(), cm: cm.clone(), surface: surface.clone(), alive: alive.clone() });
}

struct Feedback {
    ctx: Rc<Ctx>,
    cm: Rc<WpColorManagementSurfaceV1>,
    surface: Rc<WlSurface>,
    alive: Rc<Cell<bool>>,
}

impl WpColorManagementSurfaceFeedbackV1Handler for Feedback {
    fn handle_preferred_changed(&mut self, slf: &Rc<WpColorManagementSurfaceFeedbackV1>, _identity: u32) {
        if self.alive.get() {
            request_preferred(slf, &self.cm, &self.surface, &self.alive, &self.ctx);
        }
    }
}

struct MirrorDesc {
    ctx: Rc<Ctx>,
    cm: Rc<WpColorManagementSurfaceV1>,
    surface: Rc<WlSurface>,
    alive: Rc<Cell<bool>>,
}

impl WpImageDescriptionV1Handler for MirrorDesc {
    fn handle_ready(&mut self, slf: &Rc<WpImageDescriptionV1>, identity: u32) {
        // The answer may arrive after the app destroyed the surface; touching
        // the dead objects then would be a fatal protocol error.
        if !self.alive.get() {
            slf.send_destroy();
            return;
        }
        self.cm.send_set_image_description(slf, intent(&self.ctx.cfg));
        apply_now(&self.surface);
        eprintln!("[cm-shim] surface <- compositor description id {identity}");
        slf.send_destroy(); // copy semantics: safe to drop right away
    }
    fn handle_failed(&mut self, slf: &Rc<WpImageDescriptionV1>, _cause: WpImageDescriptionV1Cause, msg: &str) {
        eprintln!("[cm-shim] could not get preferred description: {msg}");
        slf.send_destroy();
    }
}

struct Surface {
    ctx: Rc<Ctx>,
    cm: Rc<WpColorManagementSurfaceV1>,
    feedback: Option<Rc<WpColorManagementSurfaceFeedbackV1>>,
    alive: Rc<Cell<bool>>,
}

impl WlSurfaceHandler for Surface {
    fn handle_destroy(&mut self, slf: &Rc<WlSurface>) {
        self.alive.set(false);
        self.ctx.waiting.borrow_mut().retain(|(_, s)| !Rc::ptr_eq(s, slf));
        if let Some(fb) = self.feedback.take() {
            fb.unset_handler();
            fb.send_destroy();
        }
        self.cm.send_destroy();
        slf.send_destroy();
    }
}

// ------------------------------------------------------ install/uninstall ---
//
// A .desktop file in ~/.local/share/applications overrides the system one with
// the same file name. `install` writes such a copy with every Exec= line
// routed through the shim; `uninstall` removes it again. Only files carrying
// MARKER are ever overwritten or deleted.

const MARKER: &str = "# cm-shim override of ";

fn user_apps_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .unwrap_or_else(|_| format!("{}/.local/share", std::env::var("HOME").unwrap_or_default()));
    Path::new(&base).join("applications")
}

fn desktop_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "desktop"))
        .collect();
    v.sort();
    v
}

fn stem(p: &Path) -> String {
    p.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

/// Basename of the program in the first Exec= line.
fn exec_program(text: &str) -> Option<String> {
    let line = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let first = line.split_whitespace().next()?.trim_matches('"');
    Some(Path::new(first).file_name()?.to_string_lossy().into_owned())
}

fn is_ours(p: &Path) -> bool {
    std::fs::read_to_string(p).is_ok_and(|t| t.starts_with(MARKER))
}

/// Pick exactly one file, or stop and let the user be more specific.
fn pick(app: &str, mut found: Vec<PathBuf>, what: &str) -> PathBuf {
    match found.len() {
        1 => found.remove(0),
        0 => die(&format!("no {what} matches '{app}'")),
        _ => {
            eprintln!("[cm-shim] '{app}' matches several; use one of these names:");
            for p in &found {
                eprintln!("    {}", stem(p));
            }
            std::process::exit(1);
        }
    }
}

fn find_system_desktop(app: &str) -> PathBuf {
    let user = user_apps_dir();
    let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    // Earlier directories take precedence, as in the desktop entry spec.
    let mut all: Vec<PathBuf> = Vec::new();
    for d in dirs.split(':').filter(|d| !d.is_empty()) {
        let d = Path::new(d).join("applications");
        if d == user {
            continue;
        }
        for p in desktop_files(&d) {
            if !all.iter().any(|q| q.file_name() == p.file_name()) {
                all.push(p);
            }
        }
    }
    if let Some(p) = all.iter().find(|p| stem(p) == app || p.file_name().is_some_and(|f| f == app)) {
        return p.clone();
    }
    let needle = app.to_lowercase();
    let found = all
        .into_iter()
        .filter(|p| {
            stem(p).to_lowercase().contains(&needle)
                || std::fs::read_to_string(p).ok().and_then(|t| exec_program(&t)).is_some_and(|e| e == app)
        })
        .collect();
    pick(app, found, "installed application")
}

fn install(app: &str, flags: &[String]) {
    let src = find_system_desktop(app);
    let dst = user_apps_dir().join(src.file_name().unwrap());
    if dst.exists() && !is_ours(&dst) {
        die(&format!("{} already exists and was not made by cm-shim; not touching it", dst.display()));
    }
    let text = std::fs::read_to_string(&src)
        .unwrap_or_else(|e| die(&format!("cannot read {}: {e}", src.display())));
    let shim = std::env::current_exe()
        .unwrap_or_else(|e| die(&format!("cannot find my own path: {e}")))
        .to_string_lossy()
        .into_owned();
    let shim = if shim.contains(char::is_whitespace) { format!("\"{shim}\"") } else { shim };
    let mut prefix = shim;
    for f in flags {
        prefix.push(' ');
        prefix.push_str(f);
    }

    let mut out = format!("{MARKER}{}\n", src.display());
    for line in text.lines() {
        if let Some(cmd) = line.strip_prefix("Exec=") {
            out.push_str(&format!("Exec={prefix} run {cmd}\n"));
        } else if line.starts_with("DBusActivatable=") {
            // D-Bus activation would start the app without ever reading Exec.
            out.push_str("DBusActivatable=false\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    std::fs::create_dir_all(dst.parent().unwrap())
        .and_then(|_| std::fs::write(&dst, &out))
        .unwrap_or_else(|e| die(&format!("cannot write {}: {e}", dst.display())));

    println!("source  : {}", src.display());
    println!("override: {}", dst.display());
    for l in out.lines().filter(|l| l.starts_with("Exec=")) {
        println!("  {l}");
    }
}

fn uninstall(app: &str) {
    let ours: Vec<PathBuf> = desktop_files(&user_apps_dir()).into_iter().filter(|p| is_ours(p)).collect();
    let needle = app.to_lowercase();
    let exact = ours.iter().find(|p| stem(p) == app).cloned();
    let target = match exact {
        Some(p) => p,
        None => {
            let found = ours
                .into_iter()
                .filter(|p| {
                    stem(p).to_lowercase().contains(&needle)
                        || std::fs::read_to_string(p).ok().is_some_and(|t| {
                            t.lines().any(|l| l.starts_with("Exec=") && l.contains(&format!(" run {app}")))
                        })
                })
                .collect();
            pick(app, found, "cm-shim override")
        }
    };
    std::fs::remove_file(&target)
        .unwrap_or_else(|e| die(&format!("cannot remove {}: {e}", target.display())));
    println!("removed : {}", target.display());
}

// ------------------------------------------------------------------- main ---

fn main() {
    const USAGE: &str = "usage: cm-shim [-s|--space SPACE] [-i|--intent INTENT] run <app> [args...]\n       cm-shim [-s|--space SPACE] [-i|--intent INTENT] install <app>\n       cm-shim uninstall <app>";
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = load_config();
    // Flags come before `run` and override the config file. Everything after
    // `run` belongs to the app and is never interpreted.
    let mut flags: Vec<String> = Vec::new(); // as typed, for `install`
    loop {
        let key = match args.first().map(String::as_str) {
            Some("run") | Some("install") | Some("uninstall") => break,
            Some("-s") | Some("--space") => "space",
            Some("-i") | Some("--intent") => "intent",
            _ => die(USAGE),
        };
        if args.len() < 2 {
            die(USAGE);
        }
        apply_setting(&mut cfg, key, &args[1]); // also validates the value
        flags.extend(args.drain(..2));
    }
    if args.len() < 2 {
        die(USAGE);
    }
    match args[0].as_str() {
        "install" if args.len() == 2 => return install(&args[1], &flags),
        "uninstall" if args.len() == 2 => return uninstall(&args[1]),
        "install" | "uninstall" => die(USAGE),
        _ => {}
    }
    let proxy = SimpleProxy::new(Baseline::ALL_OF_THEM)
        .unwrap_or_else(|e| die(&format!("cannot start proxy: {e}")));
    Command::new(&args[1])
        .args(&args[2..])
        .with_wayland_display(proxy.display())
        .spawn_and_forward_exit_code()
        .unwrap_or_else(|e| die(&format!("cannot start {}: {e}", args[1])));
    proxy.run(move || Display { cfg });
}

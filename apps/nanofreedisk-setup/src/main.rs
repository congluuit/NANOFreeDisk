//! NANOFreeDisk branded setup: wizard (egui), silent install and uninstaller.
//! See `docs/PHASE8_INSTALLERS.md` §2.4 and the crate docs.

// No console window: this is a GUI program (silent mode reports by exit code).
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("nanofreedisk-setup only runs on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(app::run(&nanofreedisk_setup::args::parse(&args)));
}

#[cfg(windows)]
mod app {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver};
    use std::time::{Duration, Instant};

    use eframe::egui::{
        self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Layout,
        Margin, RichText, Stroke, TextureHandle, Vec2,
    };
    use nanofreedisk_setup::args::{self, Args};
    use nanofreedisk_setup::install::{self, Outcome, Plan, Problem, Step};
    use nanofreedisk_setup::payload::{self, Entry};
    use nanofreedisk_setup::sys;
    use nanofreedisk_setup::text::{Lang, Text, fill};
    use nanofreedisk_setup::{APP_EXE, VERSION};

    // exit codes (documented in docs/PHASE8_INSTALLERS.md §2.4)
    const OK: i32 = 0;
    const FAILED: i32 = 1;
    const CANCELLED: i32 = 2;
    const NEEDS_ADMIN: i32 = 3;
    const FOREIGN: i32 = 4;
    const RUNNING: i32 = 5;

    // docs/BRAND.md
    const NAVY: Color32 = Color32::from_rgb(0x0B, 0x12, 0x20);
    const SURFACE: Color32 = Color32::from_rgb(0x11, 0x1C, 0x33);
    const RAISED: Color32 = Color32::from_rgb(0x1A, 0x27, 0x44);
    const CYAN: Color32 = Color32::from_rgb(0x22, 0xD3, 0xEE);
    const LIME: Color32 = Color32::from_rgb(0xA3, 0xE6, 0x35);
    const AMBER: Color32 = Color32::from_rgb(0xF5, 0x9E, 0x0B);
    const TEXT: Color32 = Color32::from_rgb(0xE6, 0xED, 0xF7);
    const MUTED: Color32 = Color32::from_rgb(0x9A, 0xA8, 0xBF);

    const LICENSE: &str = include_str!("../../../LICENSE");
    const LOGO_PNG: &[u8] = include_bytes!("../../../docs/assets/brand/icon-256.png");
    const README_URL: &str = "https://github.com/congluuit/NANOFreeDisk#readme";

    fn code_for(p: &Problem) -> i32 {
        match p {
            Problem::NeedsAdmin | Problem::NotWritable(_) => NEEDS_ADMIN,
            Problem::Foreign(_) => FOREIGN,
            Problem::Running(_) => RUNNING,
            Problem::NoPayload | Problem::Io(_) => FAILED,
        }
    }

    fn describe(t: &Text, p: &Problem) -> String {
        match p {
            Problem::NeedsAdmin => t.error_admin.to_owned(),
            Problem::NotWritable(dir) => {
                fill(t.error_not_writable, &[("dir", &dir.display().to_string())])
            }
            Problem::NoPayload => t.error_payload.to_owned(),
            Problem::Running(names) => format!("{}\n\n{}", t.error_running, names.join(", ")),
            Problem::Foreign(e) => fill(
                t.error_foreign,
                &[
                    ("version", &e.version),
                    (
                        "dir",
                        &e.dir
                            .as_ref()
                            .map_or_else(String::new, |d| d.display().to_string()),
                    ),
                ],
            ),
            Problem::Io(msg) => msg.clone(),
        }
    }

    fn system_lang() -> Lang {
        use windows::Win32::Globalization::GetUserDefaultUILanguage;
        // SAFETY: no arguments.
        let id = unsafe { GetUserDefaultUILanguage() };
        if id & 0x3ff == 0x2a {
            Lang::Vi
        } else {
            Lang::En
        }
    }

    fn load_payload() -> Result<Vec<Entry>, Problem> {
        let exe = std::env::current_exe().map_err(|e| Problem::Io(e.to_string()))?;
        let bytes = std::fs::read(&exe).map_err(|e| Problem::Io(e.to_string()))?;
        let packed = payload::find(&bytes).ok_or(Problem::NoPayload)?;
        payload::decompress(packed).map_err(|e| Problem::Io(e.to_string()))
    }

    fn plan_from(a: &Args, lang: Lang) -> Plan {
        Plan {
            dir: a
                .dir
                .clone()
                .unwrap_or_else(|| sys::default_dir(a.all_users)),
            all_users: a.all_users,
            start_menu: a.start_menu,
            desktop: a.desktop,
            shell_menu: a.shell_menu,
            lang,
        }
    }

    /// The folder this uninstaller belongs to.
    fn uninstall_dir() -> Option<PathBuf> {
        let here = std::env::current_exe().ok()?.parent()?.to_path_buf();
        if here.join(install::MANIFEST).is_file() {
            return Some(here);
        }
        install::ours().and_then(|e| e.dir)
    }

    fn manifest_all_users(dir: &Path) -> bool {
        std::fs::read_to_string(dir.join(install::MANIFEST))
            .is_ok_and(|s| s.lines().any(|l| l == "all_users=1"))
    }

    /// Re-run this exe elevated with `params`; `None` when UAC was refused.
    fn relaunch_elevated(params: &[String]) -> Option<u32> {
        let exe = std::env::current_exe().ok()?;
        let line: Vec<String> = params.iter().map(|p| sys::quote(p)).collect();
        sys::shell_execute(&exe, &line.join(" "), "runas", true).ok()
    }

    pub fn run(a: &Args) -> i32 {
        let lang = a.lang.unwrap_or_else(system_lang);
        if a.uninstall {
            let Some(dir) = uninstall_dir() else {
                return FAILED;
            };
            if manifest_all_users(&dir) && !sys::is_elevated() && !a.elevated {
                let mut params: Vec<String> = std::env::args().skip(1).collect();
                params.push("--elevated".into());
                return relaunch_elevated(&params).map_or(NEEDS_ADMIN, |c| c as i32);
            }
            if a.silent {
                return match install::uninstall(&dir, a.remove_data, true, lang) {
                    Ok(()) => OK,
                    Err(p) => code_for(&p),
                };
            }
            return gui(Wizard::uninstaller(lang, dir));
        }
        if a.update {
            return update(a, lang);
        }
        if a.silent {
            let plan = plan_from(a, lang);
            if plan.all_users && !sys::is_elevated() {
                return NEEDS_ADMIN;
            }
            let entries = match load_payload() {
                Ok(e) => e,
                Err(p) => return code_for(&p),
            };
            return match install::install(&plan, &entries, &mut |_| {}) {
                Ok(_) => OK,
                Err(p) => code_for(&p),
            };
        }
        gui(Wizard::installer(lang, a))
    }

    /// The app's "Update now" (`--update`): wait for the app to close, install over
    /// its folder with the options it was installed with, start it again. No wizard;
    /// a failure is shown in a message box because nothing else is on screen.
    fn update(a: &Args, lang: Lang) -> i32 {
        let t = lang.text();
        let fail = |detail: &str| {
            message_box(t.window_title, &format!("{}\n\n{detail}", t.update_failed));
        };
        let Some(dir) = a
            .dir
            .clone()
            .or_else(|| install::ours().and_then(|e| e.dir))
        else {
            fail(t.error_payload);
            return FAILED;
        };
        let Some(plan) = install::update_plan(&dir, lang) else {
            fail(&format!("{} ({})", install::MANIFEST, dir.display()));
            return FAILED;
        };
        if let Some(pid) = a.wait_pid {
            sys::wait_for_exit(pid, Duration::from_secs(60));
        }
        // the app's helper processes may need a moment longer
        let deadline = Instant::now() + Duration::from_secs(30);
        while !sys::running_in(&dir).is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(300));
        }
        let code = if plan.all_users && !sys::is_elevated() {
            // one Windows prompt; the elevated copy neither waits nor relaunches
            let params = vec![
                "--update".to_owned(),
                format!("/LANG={}", lang.code()),
                format!("/D={}", dir.display()),
            ];
            let code = relaunch_elevated(&params).map_or(NEEDS_ADMIN, |c| c as i32);
            if code == NEEDS_ADMIN {
                fail(t.error_admin);
            }
            code
        } else {
            match load_payload().and_then(|e| install::install(&plan, &e, &mut |_| {})) {
                Ok(_) => OK,
                Err(p) => {
                    fail(&describe(t, &p));
                    code_for(&p)
                }
            }
        };
        // not from the elevated copy: the app must not run as administrator
        if a.relaunch {
            let _ = sys::shell_execute(&dir.join(APP_EXE), "", "open", false);
        }
        code
    }

    fn gui(wizard: Wizard) -> i32 {
        let icon = decode_png(LOGO_PNG).map(|(w, h, rgba)| egui::IconData {
            rgba,
            width: w,
            height: h,
        });
        let mut viewport = egui::ViewportBuilder::default()
            .with_inner_size([680.0, 480.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_title(wizard.title());
        if let Some(icon) = icon {
            viewport = viewport.with_icon(icon);
        }
        let options = eframe::NativeOptions {
            viewport,
            centered: true,
            ..Default::default()
        };
        let code = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(CANCELLED));
        let code2 = code.clone();
        let title = wizard.title();
        let res = eframe::run_native(
            &title,
            options,
            Box::new(move |cc| {
                setup_style(&cc.egui_ctx);
                Ok(Box::new(App {
                    w: wizard,
                    code: code2,
                    logo: None,
                    shots: std::env::var_os("NFD_SETUP_SHOTS").map(|d| Shots {
                        dir: PathBuf::from(d),
                        index: 0,
                        frames: 0,
                        requested: false,
                    }),
                }))
            }),
        );
        if let Err(e) = res {
            // no usable graphics (very old driver, remote session without GPU…)
            let msg = format!(
                "{}\n\n{e}\n\nSilent install: NANOFreeDisk-Setup.exe /S\nOr use the NSIS / MSI package.",
                title
            );
            message_box(&title, &msg);
            return FAILED;
        }
        code.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn message_box(title: &str, text: &str) {
        use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW};
        use windows::core::HSTRING;
        // SAFETY: both strings live across the call; no owner window.
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(text),
                &HSTRING::from(title),
                MB_ICONERROR,
            );
        }
    }

    fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
        let mut reader = dec.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).ok()?;
        buf.truncate(info.buffer_size());
        if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
            return None;
        }
        Some((info.width, info.height, buf))
    }

    fn setup_style(ctx: &egui::Context) {
        // Windows' own fonts (Segoe UI, as in docs/BRAND.md): nothing to bundle
        let windir =
            std::env::var_os("WINDIR").map_or_else(|| PathBuf::from("C:/Windows"), PathBuf::from);
        let mut fonts = FontDefinitions::empty();
        let mut family = |fam: FontFamily, files: &[&str]| {
            let mut names = Vec::new();
            for file in files {
                if let Ok(bytes) = std::fs::read(windir.join("Fonts").join(file)) {
                    fonts.font_data.insert(
                        (*file).to_owned(),
                        std::sync::Arc::new(FontData::from_owned(bytes)),
                    );
                    names.push((*file).to_owned());
                }
            }
            fonts.families.insert(fam, names);
        };
        // later files cover glyphs the first one lacks (symbols, emoji)
        let regular = ["segoeui.ttf", "arial.ttf", "seguisym.ttf", "seguiemj.ttf"];
        family(FontFamily::Proportional, &regular);
        family(
            FontFamily::Monospace,
            &["consola.ttf", "cour.ttf", "segoeui.ttf", "seguisym.ttf"],
        );
        family(
            FontFamily::Name("bold".into()),
            &["segoeuib.ttf", "arialbd.ttf", "segoeui.ttf", "seguisym.ttf"],
        );
        ctx.set_fonts(fonts);
        // brand colors are dark whatever the Windows theme is
        ctx.set_theme(egui::ThemePreference::Dark);

        let mut style = (*ctx.style()).clone();
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = SURFACE;
        v.window_fill = SURFACE;
        v.extreme_bg_color = NAVY;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = CYAN;
        v.selection.stroke = Stroke::new(1.0_f32, NAVY);
        v.hyperlink_color = CYAN;
        v.widgets.inactive.bg_fill = RAISED;
        v.widgets.inactive.weak_bg_fill = RAISED;
        // outlines keep checkboxes and radio buttons visible on the navy surface
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x4A, 0x5A, 0x7A));
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x24, 0x34, 0x58);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(0x2C, 0x40, 0x6A);
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.corner_radius = CornerRadius::same(6);
        }
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(14.0, 6.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.5));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.5));
        style.text_styles.insert(
            egui::TextStyle::Heading,
            FontId::new(22.0, FontFamily::Name("bold".into())),
        );
        style
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.5));
        ctx.set_style(style);
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Page {
        Splash,
        Welcome,
        License,
        Kind,
        Options,
        Elevating,
        Progress,
        Done,
        Failed,
        UnConfirm,
        UnProgress,
        UnDone,
    }

    enum Msg {
        Step(Step),
        Done(Result<Outcome, Problem>),
        Elevated(Option<u32>),
        Uninstalled(Result<(), Problem>),
    }

    #[allow(clippy::struct_excessive_bools)]
    struct Wizard {
        lang: Lang,
        page: Page,
        started: Instant,
        accepted: bool,
        plan: Plan,
        /// Folder typed by the user (kept apart from the default of each kind).
        custom_dir: Option<PathBuf>,
        entries: Result<Vec<Entry>, Problem>,
        step: Option<Step>,
        rx: Option<Receiver<Msg>>,
        error: Option<Problem>,
        outcome: Outcome,
        launch: bool,
        readme: bool,
        /// Started elevated by the wizard: finish without the Done page.
        elevated_child: bool,
        remove_data: bool,
        un_dir: Option<PathBuf>,
        foreign_uninstall: Option<String>,
    }

    impl Wizard {
        fn installer(lang: Lang, a: &Args) -> Self {
            let mut a = a.clone();
            // an earlier install of ours: same kind and folder (upgrade)
            if let (Some(prev), false) = (install::ours(), a.elevated) {
                a.all_users = prev.all_users;
                if a.dir.is_none() {
                    a.dir = prev.dir;
                }
            }
            if !a.elevated {
                a.desktop = true;
                a.shell_menu = true;
            }
            let custom_dir = a.dir.clone();
            let mut w = Self {
                lang,
                page: if a.elevated {
                    Page::Progress
                } else {
                    Page::Splash
                },
                started: Instant::now(),
                accepted: a.elevated,
                plan: plan_from(&a, lang),
                custom_dir,
                entries: load_payload(),
                step: None,
                rx: None,
                error: None,
                outcome: Outcome::default(),
                launch: true,
                readme: false,
                elevated_child: a.elevated,
                remove_data: false,
                un_dir: None,
                foreign_uninstall: None,
            };
            if a.elevated {
                w.start_install();
            }
            w
        }

        fn uninstaller(lang: Lang, dir: PathBuf) -> Self {
            let mut w = Self::installer(lang, &Args::default());
            w.page = Page::UnConfirm;
            w.un_dir = Some(dir);
            w
        }

        fn t(&self) -> &'static Text {
            self.lang.text()
        }

        fn title(&self) -> String {
            if self.un_dir.is_some() {
                self.t().uninstall_title.to_owned()
            } else {
                self.t().window_title.to_owned()
            }
        }

        fn start_install(&mut self) {
            let entries = match &self.entries {
                Ok(e) => e.clone(),
                Err(p) => {
                    self.fail(p.clone());
                    return;
                }
            };
            let (tx, rx) = mpsc::channel();
            let plan = self.plan.clone();
            std::thread::spawn(move || {
                let tx2 = tx.clone();
                let res = install::install(&plan, &entries, &mut |s| {
                    let _ = tx2.send(Msg::Step(s));
                });
                let _ = tx.send(Msg::Done(res));
            });
            self.rx = Some(rx);
            self.page = Page::Progress;
        }

        fn start_elevated(&mut self) {
            let mut a = Args {
                all_users: true,
                dir: Some(self.plan.dir.clone()),
                start_menu: self.plan.start_menu,
                desktop: self.plan.desktop,
                shell_menu: self.plan.shell_menu,
                lang: Some(self.lang),
                ..Args::default()
            };
            a.elevated = true;
            let params = args::to_command_line(&a);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(Msg::Elevated(relaunch_elevated(&params)));
            });
            self.rx = Some(rx);
            self.page = Page::Elevating;
        }

        fn start_uninstall(&mut self) {
            let Some(dir) = self.un_dir.clone() else {
                return;
            };
            let (tx, rx) = mpsc::channel();
            let (remove, lang) = (self.remove_data, self.lang);
            std::thread::spawn(move || {
                let _ = tx.send(Msg::Uninstalled(install::uninstall(
                    &dir, remove, false, lang,
                )));
            });
            self.rx = Some(rx);
            self.page = Page::UnProgress;
        }

        fn fail(&mut self, p: Problem) {
            self.foreign_uninstall = match &p {
                Problem::Foreign(e) if !e.uninstall.is_empty() => Some(e.uninstall.clone()),
                _ => None,
            };
            self.error = Some(p);
            self.page = Page::Failed;
        }

        fn poll(&mut self, code: &std::sync::atomic::AtomicI32, ctx: &egui::Context) {
            let Some(rx) = &self.rx else {
                return;
            };
            let mut msgs = Vec::new();
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
            for m in msgs {
                match m {
                    Msg::Step(s) => self.step = Some(s),
                    Msg::Done(Ok(outcome)) => {
                        self.outcome = outcome;
                        self.rx = None;
                        code.store(OK, std::sync::atomic::Ordering::SeqCst);
                        if self.elevated_child {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        self.page = Page::Done;
                    }
                    Msg::Done(Err(p)) => {
                        self.rx = None;
                        code.store(code_for(&p), std::sync::atomic::Ordering::SeqCst);
                        self.fail(p);
                    }
                    Msg::Elevated(res) => {
                        self.rx = None;
                        match res {
                            Some(0) => {
                                code.store(OK, std::sync::atomic::Ordering::SeqCst);
                                self.page = Page::Done;
                            }
                            Some(c) => {
                                code.store(c as i32, std::sync::atomic::Ordering::SeqCst);
                                self.error = Some(Problem::Io(format!("exit code {c}")));
                                self.page = Page::Failed;
                            }
                            // UAC refused: back to the options
                            None => self.page = Page::Options,
                        }
                    }
                    Msg::Uninstalled(res) => {
                        self.rx = None;
                        match res {
                            Ok(()) => {
                                code.store(OK, std::sync::atomic::Ordering::SeqCst);
                                self.page = Page::UnDone;
                            }
                            Err(p) => {
                                code.store(code_for(&p), std::sync::atomic::Ordering::SeqCst);
                                self.fail(p);
                            }
                        }
                    }
                }
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        fn finish(&self) {
            if self.launch {
                let exe = self.plan.dir.join(APP_EXE);
                // through Explorer, so the app never inherits administrator rights
                let _ = std::process::Command::new("explorer.exe").arg(&exe).spawn();
            }
            if self.readme {
                let _ = sys::shell_execute(Path::new(README_URL), "", "open", false);
            }
        }
    }

    struct App {
        w: Wizard,
        code: std::sync::Arc<std::sync::atomic::AtomicI32>,
        logo: Option<TextureHandle>,
        shots: Option<Shots>,
    }

    /// Developer aid: `NFD_SETUP_SHOTS=<dir>` walks through every page, saves a PNG
    /// of each and exits (nothing is installed). Used to review the design.
    struct Shots {
        dir: PathBuf,
        index: usize,
        frames: u32,
        requested: bool,
    }

    const SHOT_PAGES: [Page; 10] = [
        Page::Splash,
        Page::Welcome,
        Page::License,
        Page::Kind,
        Page::Options,
        Page::Progress,
        Page::Done,
        Page::Failed,
        Page::UnConfirm,
        Page::UnDone,
    ];

    fn save_png(path: &Path, image: &egui::ColorImage) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let [w, h] = image.size;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(std::io::Error::other)?;
        let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
        writer
            .write_image_data(&bytes)
            .map_err(std::io::Error::other)
    }

    impl App {
        fn shoot(&mut self, ctx: &egui::Context) {
            let Some(shots) = self.shots.as_mut().filter(|s| s.index < SHOT_PAGES.len()) else {
                return;
            };
            let image = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = image {
                let page = SHOT_PAGES[shots.index];
                let name = format!("{:02}-{}-{page:?}.png", shots.index, self.w.lang.code());
                let _ = save_png(&shots.dir.join(name), &image);
                shots.index += 1;
                shots.requested = false;
                shots.frames = 0;
                if shots.index >= SHOT_PAGES.len() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            }
            let page = SHOT_PAGES[shots.index];
            if self.w.page != page {
                self.w.page = page;
                match page {
                    Page::Progress => self.w.step = Some(Step::Files),
                    Page::Failed => {
                        self.w.error = Some(Problem::Foreign(install::Existing {
                            ours: false,
                            all_users: false,
                            dir: Some(PathBuf::from(r"C:\Users\you\AppData\Local\NANOFreeDisk")),
                            version: VERSION.into(),
                            uninstall: "uninstall.exe".into(),
                        }));
                        self.w.foreign_uninstall = Some("uninstall.exe".into());
                    }
                    Page::UnConfirm | Page::UnDone => {
                        self.w.un_dir = Some(self.w.plan.dir.clone());
                    }
                    _ => {}
                }
            }
            shots.frames += 1;
            if !shots.requested && shots.frames > 5 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                shots.requested = true;
            }
            ctx.request_repaint();
        }
    }

    impl eframe::App for App {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.w.poll(&self.code, ctx);
            self.shoot(ctx);
            if self.w.page == Page::Splash && self.shots.is_none() {
                if self.w.started.elapsed() > Duration::from_millis(1200) {
                    self.w.page = Page::Welcome;
                } else {
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
            }
            if self.logo.is_none() {
                if let Some((w, h, rgba)) = decode_png(LOGO_PNG) {
                    let img =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    self.logo = Some(ctx.load_texture("logo", img, egui::TextureOptions::LINEAR));
                }
            }
            if self.w.page == Page::Splash {
                self.splash(ctx);
                return;
            }
            self.header(ctx);
            self.footer(ctx);
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(SURFACE)
                        .inner_margin(Margin::symmetric(32, 20)),
                )
                .show(ctx, |ui| self.body(ui));
        }
    }

    fn gradient_bar(ui: &egui::Ui, rect: egui::Rect) {
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), CYAN);
        mesh.colored_vertex(rect.right_top(), LIME);
        mesh.colored_vertex(rect.right_bottom(), LIME);
        mesh.colored_vertex(rect.left_bottom(), CYAN);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().add(egui::Shape::mesh(mesh));
    }

    fn wordmark(ui: &mut egui::Ui, size: f32) {
        let bold = FontFamily::Name("bold".into());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(
                RichText::new("NANO")
                    .font(FontId::new(size, bold.clone()))
                    .color(Color32::WHITE),
            );
            ui.label(
                RichText::new("FreeDisk")
                    .font(FontId::new(size, bold))
                    .color(CYAN),
            );
        });
    }

    impl App {
        fn splash(&self, ctx: &egui::Context) {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(NAVY))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(110.0);
                        if let Some(logo) = &self.logo {
                            ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(112.0)));
                        }
                        ui.add_space(14.0);
                        ui.scope(|ui| {
                            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_space((ui.available_width() - 230.0).max(0.0) / 2.0);
                                    wordmark(ui, 34.0);
                                });
                            });
                        });
                        ui.label(RichText::new(self.w.t().tagline).color(MUTED));
                        let r = ui.max_rect();
                        let bar = egui::Rect::from_min_size(
                            egui::pos2(r.left(), r.bottom() - 4.0),
                            Vec2::new(r.width(), 4.0),
                        );
                        gradient_bar(ui, bar);
                    });
                });
        }

        fn header(&mut self, ctx: &egui::Context) {
            egui::TopBottomPanel::top("header")
                .exact_height(84.0)
                .frame(
                    egui::Frame::new()
                        .fill(NAVY)
                        .inner_margin(Margin::symmetric(24, 14)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(logo) = &self.logo {
                            ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(52.0)));
                        }
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            wordmark(ui, 24.0);
                            ui.label(RichText::new(self.w.t().tagline).small().color(MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let busy = matches!(
                                self.w.page,
                                Page::Progress | Page::UnProgress | Page::Elevating
                            );
                            ui.add_enabled_ui(!busy, |ui| {
                                for (lang, label) in [(Lang::Vi, "VI"), (Lang::En, "EN")] {
                                    if ui.selectable_label(self.w.lang == lang, label).clicked() {
                                        self.w.lang = lang;
                                        self.w.plan.lang = lang;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                            self.w.title(),
                                        ));
                                    }
                                }
                            });
                            ui.label(RichText::new(format!("v{VERSION}")).small().color(MUTED));
                        });
                    });
                    let r = ui.max_rect();
                    let bar = egui::Rect::from_min_size(
                        egui::pos2(r.left() - 24.0, r.bottom() + 11.0),
                        Vec2::new(r.width() + 48.0, 3.0),
                    );
                    gradient_bar(ui, bar);
                });
        }

        fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
            let b = egui::Button::new(RichText::new(label).color(NAVY).strong())
                .fill(if enabled { CYAN } else { RAISED })
                .min_size(Vec2::new(110.0, 32.0));
            ui.add_enabled(enabled, b).clicked()
        }

        fn secondary(ui: &mut egui::Ui, label: &str) -> bool {
            ui.add(egui::Button::new(label).min_size(Vec2::new(96.0, 32.0)))
                .clicked()
        }

        #[allow(clippy::too_many_lines)]
        fn footer(&mut self, ctx: &egui::Context) {
            let t = self.w.t();
            egui::TopBottomPanel::bottom("footer")
                .exact_height(60.0)
                .frame(
                    egui::Frame::new()
                        .fill(NAVY)
                        .inner_margin(Margin::symmetric(24, 14)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        // step dots
                        let order = [
                            Page::Welcome,
                            Page::License,
                            Page::Kind,
                            Page::Options,
                            Page::Progress,
                            Page::Done,
                        ];
                        if let Some(pos) = order.iter().position(|p| *p == self.w.page) {
                            for (i, _) in order.iter().enumerate() {
                                let (rect, _) = ui.allocate_exact_size(
                                    Vec2::new(18.0, 8.0),
                                    egui::Sense::hover(),
                                );
                                let color = if i <= pos { CYAN } else { RAISED };
                                ui.painter().circle_filled(rect.center(), 4.0, color);
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let w = &mut self.w;
                            match w.page {
                                Page::Welcome => {
                                    if Self::primary(ui, t.next, true) {
                                        w.page = Page::License;
                                    }
                                    if Self::secondary(ui, t.cancel) {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                }
                                Page::License => {
                                    if Self::primary(ui, t.next, w.accepted) {
                                        w.page = Page::Kind;
                                    }
                                    if Self::secondary(ui, t.back) {
                                        w.page = Page::Welcome;
                                    }
                                }
                                Page::Kind => {
                                    if Self::primary(ui, t.next, true) {
                                        w.page = Page::Options;
                                    }
                                    if Self::secondary(ui, t.back) {
                                        w.page = Page::License;
                                    }
                                }
                                Page::Options => {
                                    let label = if w.plan.all_users && !sys::is_elevated() {
                                        format!("🛡 {}", t.install)
                                    } else {
                                        t.install.to_owned()
                                    };
                                    if Self::primary(ui, &label, true) {
                                        if w.plan.all_users && !sys::is_elevated() {
                                            w.start_elevated();
                                        } else {
                                            w.start_install();
                                        }
                                    }
                                    if Self::secondary(ui, t.back) {
                                        w.page = Page::Kind;
                                    }
                                }
                                Page::Done => {
                                    if Self::primary(ui, t.finish, true) {
                                        w.finish();
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                }
                                Page::Failed => {
                                    if Self::primary(ui, t.close, true) {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                    if w.un_dir.is_none() && Self::secondary(ui, t.back) {
                                        w.error = None;
                                        w.page = Page::Options;
                                    }
                                }
                                Page::UnConfirm => {
                                    if Self::primary(ui, t.un_button, true) {
                                        w.start_uninstall();
                                    }
                                    if Self::secondary(ui, t.cancel) {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                }
                                Page::UnDone => {
                                    if Self::primary(ui, t.close, true) {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                }
                                Page::Splash
                                | Page::Progress
                                | Page::UnProgress
                                | Page::Elevating => {}
                            }
                        });
                    });
                });
        }

        fn title(ui: &mut egui::Ui, text: &str) {
            ui.heading(text);
            ui.add_space(6.0);
        }

        fn card(
            ui: &mut egui::Ui,
            selected: bool,
            add: impl FnOnce(&mut egui::Ui),
        ) -> egui::Response {
            let frame = egui::Frame::new()
                .fill(if selected { RAISED } else { SURFACE })
                .stroke(Stroke::new(1.5_f32, if selected { CYAN } else { RAISED }))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(12));
            let r = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    add(ui);
                })
                .response;
            r.interact(egui::Sense::click())
        }

        #[allow(clippy::too_many_lines)]
        fn body(&mut self, ui: &mut egui::Ui) {
            let t = self.w.t();
            let w = &mut self.w;
            match w.page {
                Page::Splash => {}
                Page::Welcome => {
                    Self::title(ui, t.welcome_title);
                    ui.label(fill(t.welcome_body, &[("version", VERSION)]));
                    if let Err(p) = &w.entries {
                        ui.add_space(8.0);
                        ui.colored_label(AMBER, describe(t, p));
                    }
                }
                Page::License => {
                    Self::title(ui, t.license_title);
                    egui::Frame::new()
                        .fill(NAVY)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(190.0)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        RichText::new(LICENSE).monospace().size(12.0).color(MUTED),
                                    );
                                });
                        });
                    ui.add_space(6.0);
                    ui.checkbox(&mut w.accepted, t.license_accept);
                }
                Page::Kind => {
                    Self::title(ui, t.type_title);
                    for (all, label, hint) in [
                        (false, t.type_me, t.type_me_hint),
                        (true, t.type_all, t.type_all_hint),
                    ] {
                        let selected = w.plan.all_users == all;
                        let r = Self::card(ui, selected, |ui| {
                            ui.horizontal(|ui| {
                                let mut sel = selected;
                                ui.radio_value(&mut sel, true, "");
                                ui.vertical(|ui| {
                                    let icon = if all { "🛡 " } else { "" };
                                    ui.label(RichText::new(format!("{icon}{label}")).strong());
                                    ui.label(RichText::new(hint).small().color(MUTED));
                                });
                            });
                        });
                        let picked = r.clicked()
                            || (r.hovered() && ui.input(|i| i.pointer.primary_clicked()));
                        if picked && w.plan.all_users != all {
                            w.plan.all_users = all;
                            if w.custom_dir.is_none() {
                                w.plan.dir = sys::default_dir(all);
                            }
                        }
                        ui.add_space(4.0);
                    }
                }
                Page::Options => {
                    Self::title(ui, t.options_title);
                    ui.label(RichText::new(t.folder).color(MUTED));
                    ui.horizontal(|ui| {
                        let mut text = w.plan.dir.display().to_string();
                        let edit = egui::TextEdit::singleline(&mut text)
                            .desired_width(ui.available_width());
                        if ui.add(edit).changed() {
                            w.plan.dir = PathBuf::from(&text);
                            w.custom_dir = Some(w.plan.dir.clone());
                        }
                    });
                    if let Ok(entries) = &w.entries {
                        let mb = install::installed_size(entries) as f64 / 1_048_576.0;
                        ui.label(
                            RichText::new(fill(
                                t.space_needed,
                                &[("size", &format!("{mb:.1} MB"))],
                            ))
                            .small()
                            .color(MUTED),
                        );
                    }
                    ui.add_space(8.0);
                    ui.checkbox(&mut w.plan.start_menu, t.opt_start_menu);
                    ui.checkbox(&mut w.plan.desktop, t.opt_desktop);
                    ui.checkbox(&mut w.plan.shell_menu, t.opt_shell_menu);
                    ui.label(RichText::new(t.opt_shell_menu_hint).small().color(MUTED));
                }
                Page::Elevating => {
                    Self::title(ui, t.progress_title);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t.elevating);
                    });
                }
                Page::Progress => {
                    Self::title(ui, t.progress_title);
                    let steps = [
                        (Step::Check, t.step_check),
                        (Step::Files, t.step_files),
                        (Step::WebView, t.step_webview),
                        (Step::WebViewDownload, t.step_webview_download),
                        (Step::Register, t.step_register),
                        (Step::Shortcuts, t.step_shortcuts),
                    ];
                    let current = w.step.and_then(|s| steps.iter().position(|(x, _)| *x == s));
                    for (i, (step, label)) in steps.iter().enumerate() {
                        if *step == Step::WebViewDownload && w.step != Some(Step::WebViewDownload) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            match current {
                                Some(c) if i < c => {
                                    ui.label(RichText::new("✔").color(LIME));
                                }
                                Some(c) if i == c => {
                                    ui.spinner();
                                }
                                _ => {
                                    ui.label(RichText::new("•").color(MUTED));
                                }
                            }
                            ui.label(*label);
                        });
                    }
                    let frac = current.map_or(0.0, |c| (c as f32 + 0.5) / steps.len() as f32);
                    ui.add_space(8.0);
                    ui.add(egui::ProgressBar::new(frac).fill(CYAN).desired_height(6.0));
                }
                Page::Done => {
                    Self::title(ui, t.done_title);
                    ui.label(fill(
                        t.done_body,
                        &[("dir", &w.plan.dir.display().to_string())],
                    ));
                    if let Some(reason) = &w.outcome.webview_warning {
                        ui.add_space(6.0);
                        ui.colored_label(AMBER, fill(t.warn_webview, &[("reason", reason)]));
                    }
                    ui.add_space(10.0);
                    ui.checkbox(&mut w.launch, t.done_launch);
                    ui.checkbox(&mut w.readme, t.done_readme);
                }
                Page::Failed => {
                    Self::title(ui, t.error_title);
                    if let Some(p) = &w.error {
                        ui.colored_label(AMBER, describe(t, p));
                    }
                    if let Some(cmd) = w.foreign_uninstall.clone() {
                        ui.add_space(8.0);
                        if ui.button(t.run_other_uninstaller).clicked() {
                            run_command_line(&cmd);
                            w.error = None;
                            w.foreign_uninstall = None;
                            w.page = Page::Options;
                        }
                    }
                }
                Page::UnConfirm => {
                    Self::title(ui, t.un_title);
                    let dir = w
                        .un_dir
                        .as_ref()
                        .map_or_else(String::new, |d| d.display().to_string());
                    ui.label(fill(t.un_body, &[("dir", &dir)]));
                    ui.add_space(10.0);
                    ui.checkbox(&mut w.remove_data, t.un_remove_data);
                    ui.label(RichText::new(t.un_remove_data_hint).small().color(MUTED));
                }
                Page::UnProgress => {
                    Self::title(ui, t.un_title);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t.un_progress);
                    });
                }
                Page::UnDone => {
                    Self::title(ui, t.un_title);
                    ui.label(if w.remove_data {
                        t.un_done
                    } else {
                        t.un_done_kept
                    });
                }
            }
        }
    }

    /// Run an `UninstallString` from the registry (`"C:\…\uninstall.exe" /x …`) and
    /// wait for it.
    fn run_command_line(cmd: &str) {
        let cmd = cmd.trim();
        let (exe, rest) = if let Some(stripped) = cmd.strip_prefix('"') {
            match stripped.split_once('"') {
                Some((exe, rest)) => (exe.to_owned(), rest.trim().to_owned()),
                None => (stripped.to_owned(), String::new()),
            }
        } else {
            match cmd.split_once(' ') {
                Some((exe, rest)) => (exe.to_owned(), rest.trim().to_owned()),
                None => (cmd.to_owned(), String::new()),
            }
        };
        let _ = sys::shell_execute(Path::new(&exe), &rest, "open", true);
    }
}

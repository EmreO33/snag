use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;

use crate::history::{Entry as HistoryEntry, History};
use crate::installer::{InstallEvent, InstallState};
use crate::jobs::{Job, JobEvent, JobOverrides, JobState};
use crate::probe::{ProbeResult, ProbeState};
use crate::remux::{RemuxEvent, RemuxOp, RemuxState};
use crate::selfupdate::{self, InstallKind, SelfUpdateEvent, SelfUpdateState};
use crate::settings::{Mode, Settings};
use crate::theme::{self, Palette};
use crate::ui;
use crate::updater::{self, UpdateEvent, UpdateState};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Setup,
    Home,
    History,
    Queue,
    Remux,
    Settings,
    Updates,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Video,
    Audio,
    Metadata,
    Processing,
    Network,
    Advanced,
}

impl SettingsTab {
    pub const ALL: &'static [SettingsTab] = &[
        SettingsTab::Appearance,
        SettingsTab::Video,
        SettingsTab::Audio,
        SettingsTab::Metadata,
        SettingsTab::Processing,
        SettingsTab::Network,
        SettingsTab::Advanced,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            SettingsTab::Appearance => "appearance",
            SettingsTab::Video => "video",
            SettingsTab::Audio => "audio",
            SettingsTab::Metadata => "metadata",
            SettingsTab::Processing => "local processing",
            SettingsTab::Network => "network",
            SettingsTab::Advanced => "advanced",
        }
    }
}

/// What the first-run screen is holding while the user works through it.
pub struct SetupState {
    pub config_dir: PathBuf,
    pub download_dir: PathBuf,
    pub detecting: bool,
    pub found_ytdlp: Option<(String, String)>,
    pub found_ffmpeg: Option<String>,
    pub install: InstallState,
    pub ffmpeg_install: InstallState,
    pub log: Vec<String>,
    pub error: Option<String>,
}

impl SetupState {
    fn new(settings: &Settings) -> Self {
        Self {
            config_dir: crate::bootstrap::config_dir(),
            download_dir: settings.processing.download_dir.clone(),
            detecting: true,
            found_ytdlp: None,
            found_ffmpeg: None,
            install: InstallState::Idle,
            ffmpeg_install: InstallState::Idle,
            log: Vec::new(),
            error: None,
        }
    }

    /// The yt-dlp Snag would end up using, whether found or just installed.
    pub fn resolved_ytdlp(&self) -> Option<(String, String)> {
        if let InstallState::Done { path, version } = &self.install {
            return Some((path.display().to_string(), version.clone()));
        }
        self.found_ytdlp.clone()
    }
}

/// Messages from the background probes the setup screen kicks off.
#[derive(Debug)]
pub enum SetupEvent {
    Detected {
        ytdlp: Option<(String, String)>,
        ffmpeg: Option<String>,
    },
    Install(InstallEvent),
    FfmpegInstall(InstallEvent),
}

/// State for the remux screen, which runs at most one ffmpeg job at a time.
pub struct RemuxUi {
    pub input: Option<PathBuf>,
    pub op: RemuxOp,
    pub container: String,
    pub audio_codec: String,
    pub gif_fps: u32,
    pub gif_width: u32,
    pub state: RemuxState,
    pub progress: f32,
    pub log: Vec<String>,
    pub child: Arc<Mutex<Option<Child>>>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl Default for RemuxUi {
    fn default() -> Self {
        Self {
            input: None,
            op: RemuxOp::Container,
            container: "mp4".into(),
            audio_codec: "copy".into(),
            gif_fps: 15,
            gif_width: 480,
            state: RemuxState::Idle,
            progress: 0.0,
            log: Vec::new(),
            child: Arc::new(Mutex::new(None)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub struct Toast {
    pub text: String,
    pub bad: bool,
    pub at: Instant,
}

pub struct SnagApp {
    pub settings: Settings,
    saved_snapshot: Settings,
    dirty_since: Option<Instant>,
    applied_appearance: crate::settings::Appearance,

    pub palette: Palette,
    /// The logo, uploaded once and tinted per theme wherever it is drawn.
    pub logo: Option<egui::TextureHandle>,
    pub view: View,
    pub settings_tab: SettingsTab,

    pub url_input: String,
    pub mode: Mode,
    /// What the link in the box turned out to be, and the choices made about it.
    pub probe: ProbeState,
    probed_url: String,
    url_changed_at: Option<Instant>,
    probe_tx: Sender<ProbeResult>,
    probe_rx: Receiver<ProbeResult>,
    pub whole_playlist: bool,
    pub height_override: Option<u32>,

    pub history: History,

    pub jobs: Vec<Job>,
    job_tx: Sender<JobEvent>,
    job_rx: Receiver<JobEvent>,

    pub remux: RemuxUi,
    remux_tx: Sender<RemuxEvent>,
    remux_rx: Receiver<RemuxEvent>,

    pub setup: SetupState,
    setup_tx: Sender<SetupEvent>,
    setup_rx: Receiver<SetupEvent>,

    pub update_state: UpdateState,
    pub ytdlp_version: String,
    pub update_log: Vec<String>,
    upd_tx: Sender<UpdateEvent>,
    upd_rx: Receiver<UpdateEvent>,

    pub app_update_state: SelfUpdateState,
    pub install_kind: InstallKind,
    pub app_update_log: Vec<String>,
    app_tx: Sender<SelfUpdateEvent>,
    app_rx: Receiver<SelfUpdateEvent>,

    pub toast: Option<Toast>,
}

impl SnagApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (settings, load_warning) = Settings::load();
        let palette = theme::palette(settings.appearance.theme, settings.appearance.accent);
        theme::apply(&cc.egui_ctx, &palette, settings.appearance.ui_scale);

        let (job_tx, job_rx) = channel();
        let (remux_tx, remux_rx) = channel();
        let (upd_tx, upd_rx) = channel();
        let (setup_tx, setup_rx) = channel();
        let (app_tx, app_rx) = channel();
        let (probe_tx, probe_rx) = channel();
        let needs_setup = !settings.setup_done;

        let logo = crate::icon::mark_image().map(|image| {
            cc.egui_ctx
                .load_texture("snag_logo", image, egui::TextureOptions::LINEAR)
        });

        let mut app = Self {
            logo,
            saved_snapshot: settings.clone(),
            applied_appearance: settings.appearance.clone(),
            palette,
            view: if needs_setup { View::Setup } else { View::Home },
            settings_tab: SettingsTab::Video,
            url_input: String::new(),
            mode: Mode::Auto,
            probe: ProbeState::Idle,
            probed_url: String::new(),
            url_changed_at: None,
            probe_tx,
            probe_rx,
            whole_playlist: false,
            height_override: None,
            history: History::load(),
            jobs: Vec::new(),
            job_tx,
            job_rx,
            remux: RemuxUi::default(),
            remux_tx,
            remux_rx,
            setup: SetupState::new(&settings),
            setup_tx,
            setup_rx,
            app_update_state: SelfUpdateState::Unknown,
            install_kind: selfupdate::detect_install_kind(),
            app_update_log: Vec::new(),
            app_tx,
            app_rx,
            update_state: UpdateState::Unknown,
            ytdlp_version: String::new(),
            update_log: Vec::new(),
            upd_tx,
            upd_rx,
            toast: None,
            dirty_since: None,
            settings,
        };

        // Surface a config we could not read before anything else.
        if let Some(warning) = load_warning {
            app.toast(warning, true);
        }

        if needs_setup {
            app.start_detection(&cc.egui_ctx);
        } else {
            app.maybe_auto_check_updates(&cc.egui_ctx);
        }
        app
    }

    // --- helpers -----------------------------------------------------------

    pub fn toast(&mut self, text: impl Into<String>, bad: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            bad,
            at: Instant::now(),
        });
    }

    pub fn mark_dirty(&mut self) {
        self.dirty_since = Some(Instant::now());
    }

    fn repainter(ctx: &egui::Context) -> impl Fn() + Send + 'static {
        let ctx = ctx.clone();
        move || ctx.request_repaint()
    }

    pub fn active_job_count(&self) -> usize {
        self.jobs.iter().filter(|j| j.state.is_active()).count()
    }

    /// Probe the link in the box once typing settles, so the preview keeps up
    /// without firing a yt-dlp process on every keystroke.
    fn pump_probe(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.probe_rx.try_recv() {
            // A reply for a link that is no longer in the box is stale.
            if result.url == self.probed_url {
                self.probe = result.state;
            }
        }

        let url = self.url_input.trim().to_string();
        if url != self.probed_url && self.url_changed_at.is_none() {
            self.url_changed_at = Some(Instant::now());
        }

        let Some(changed) = self.url_changed_at else {
            return;
        };
        if changed.elapsed().as_millis() < 500 {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        }
        self.url_changed_at = None;

        if url == self.probed_url {
            return;
        }
        self.probed_url = url.clone();
        self.whole_playlist = false;
        self.height_override = None;

        // Only one link at a time is worth previewing; a pasted batch is not.
        let single = url.lines().count() == 1 && crate::util::looks_like_url(&url);
        if !single {
            self.probe = ProbeState::Idle;
            return;
        }
        crate::probe::spawn(
            url,
            self.settings.clone(),
            self.probe_tx.clone(),
            Self::repainter(ctx),
        );
    }

    /// The probe result for the link currently in the box, if there is one.
    pub fn current_probe(&self) -> Option<&crate::probe::Probe> {
        match &self.probe {
            ProbeState::Done(p) if p.url == self.url_input.trim() => Some(p),
            _ => None,
        }
    }

    /// Queue a download for the current URL box, one job per non-empty line.
    pub fn enqueue_current(&mut self) {
        let raw = self.url_input.trim().to_string();
        if raw.is_empty() {
            self.toast("paste a link first", true);
            return;
        }

        let urls: Vec<String> = raw
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        let mut queued = 0;
        for url in urls {
            if !crate::util::looks_like_url(&url) {
                self.toast(format!("not a link: {url}"), true);
                continue;
            }
            let overrides = JobOverrides {
                whole_playlist: self.whole_playlist,
                height: self.height_override,
            };
            self.jobs.push(Job::new(url, self.mode, overrides));
            queued += 1;
        }

        if queued > 0 {
            self.url_input.clear();
            self.probe = ProbeState::Idle;
            self.probed_url.clear();
            self.whole_playlist = false;
            self.height_override = None;
            self.toast(
                if queued == 1 {
                    "queued 1 download".to_string()
                } else {
                    format!("queued {queued} downloads")
                },
                false,
            );
        }
    }

    /// Start queued jobs while we are under the concurrency limit.
    fn pump_queue(&mut self, ctx: &egui::Context) {
        let limit = self.settings.processing.max_concurrent_jobs.max(1);
        let mut running = self.active_job_count();

        for job in self.jobs.iter_mut() {
            if running >= limit {
                break;
            }
            if job.state == JobState::Queued {
                job.state = JobState::Starting;
                running += 1;
                crate::jobs::spawn(
                    job.id,
                    job.url.clone(),
                    job.mode,
                    job.overrides,
                    self.settings.clone(),
                    job.child.clone(),
                    job.cancel_flag.clone(),
                    self.job_tx.clone(),
                    Self::repainter(ctx),
                );
            }
        }
    }

    fn drain_job_events(&mut self) {
        while let Ok(ev) = self.job_rx.try_recv() {
            match ev {
                JobEvent::Title(id, t) => {
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        j.title = t;
                    }
                }
                JobEvent::Progress {
                    id,
                    downloaded,
                    total,
                    speed,
                    eta,
                } => {
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        if let Some(d) = downloaded {
                            j.downloaded = d;
                        }
                        if let Some(t) = total {
                            j.total = t;
                        }
                        j.speed = speed.unwrap_or(0.0);
                        j.eta = eta.unwrap_or(-1.0);
                    }
                }
                JobEvent::State(id, st) => {
                    let mut finished_ok = None;
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        if st == JobState::Done {
                            j.downloaded = j.total.max(j.downloaded);
                            j.speed = 0.0;
                            finished_ok = Some(j.display_name());
                        }
                        j.state = st;
                    }
                    if let Some(name) = finished_ok {
                        let short: String = name.chars().take(48).collect();
                        self.toast(format!("saved {short}"), false);
                        self.record_in_history(id);
                    }
                }
                JobEvent::Log(id, line) => {
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        if j.log.len() > 400 {
                            j.log.drain(0..200);
                        }
                        j.log.push(line);
                    }
                }
                JobEvent::File(id, path) => {
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        j.file = Some(path);
                    }
                }
            }
        }
    }

    /// Remember a finished download so it can be found again later.
    fn record_in_history(&mut self, id: u64) {
        let Some(job) = self.jobs.iter().find(|j| j.id == id) else {
            return;
        };
        let entry = HistoryEntry {
            url: job.url.clone(),
            title: job.display_name(),
            mode: job.mode,
            file: job.file.clone(),
            bytes: job.total.max(job.downloaded),
            finished_unix: crate::updater::now_unix(),
        };
        self.history.record(entry);
        if let Err(e) = self.history.save() {
            self.toast(format!("could not save history: {e}"), true);
        }
    }

    fn drain_remux_events(&mut self) {
        while let Ok(ev) = self.remux_rx.try_recv() {
            match ev {
                RemuxEvent::Progress(f) => self.remux.progress = f,
                RemuxEvent::Log(l) => {
                    if self.remux.log.len() > 200 {
                        self.remux.log.drain(0..100);
                    }
                    self.remux.log.push(l);
                }
                RemuxEvent::State(st) => {
                    match &st {
                        RemuxState::Done(p) => {
                            self.remux.progress = 1.0;
                            let name = p
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            self.toast(format!("wrote {name}"), false);
                        }
                        RemuxState::Failed(e) => {
                            let short: String = e.chars().take(80).collect();
                            self.toast(short, true);
                        }
                        _ => {}
                    }
                    self.remux.state = st;
                }
            }
        }
    }

    fn drain_update_events(&mut self) {
        while let Ok(ev) = self.upd_rx.try_recv() {
            match ev {
                UpdateEvent::Current(v) => self.ytdlp_version = v,
                UpdateEvent::Log(l) => {
                    if self.update_log.len() > 100 {
                        self.update_log.drain(0..50);
                    }
                    self.update_log.push(l);
                }
                UpdateEvent::State(st) => {
                    match &st {
                        UpdateState::Installed { version } => {
                            self.toast(format!("yt-dlp updated to {version}"), false)
                        }
                        UpdateState::Available { latest } => {
                            self.toast(format!("yt-dlp {latest} is available"), false)
                        }
                        UpdateState::Error(e) => {
                            let short: String = e.chars().take(90).collect();
                            self.toast(short, true);
                        }
                        UpdateState::Missing(_) => self.toast(
                            "yt-dlp not found. set its path in settings > advanced.",
                            true,
                        ),
                        _ => {}
                    }
                    self.update_state = st;
                }
            }
        }
    }

    fn drain_app_update_events(&mut self) {
        while let Ok(ev) = self.app_rx.try_recv() {
            match ev {
                SelfUpdateEvent::Log(l) => self.app_update_log.push(l),
                SelfUpdateEvent::State(st) => {
                    match &st {
                        SelfUpdateState::Available { latest } => {
                            self.toast(format!("snag {latest} is available"), false)
                        }
                        SelfUpdateState::RestartRequired => {
                            self.toast("update installed, restart snag to use it", false)
                        }
                        SelfUpdateState::HandedOff => {
                            self.toast("the installer is taking over", false)
                        }
                        SelfUpdateState::Error(e) => {
                            let short: String = e.chars().take(90).collect();
                            self.toast(short, true);
                        }
                        _ => {}
                    }
                    self.app_update_state = st;
                }
            }
        }
    }

    pub fn start_app_update_check(&mut self, ctx: &egui::Context) {
        if self.app_update_state.busy() {
            return;
        }
        self.app_update_log.clear();
        selfupdate::check(self.app_tx.clone(), Self::repainter(ctx));
    }

    pub fn start_app_update_install(&mut self, ctx: &egui::Context) {
        if self.app_update_state.busy() {
            return;
        }
        let SelfUpdateState::Available { latest } = self.app_update_state.clone() else {
            return;
        };
        selfupdate::install(
            latest,
            self.install_kind,
            self.app_tx.clone(),
            Self::repainter(ctx),
        );
    }

    pub fn start_update_check(&mut self, ctx: &egui::Context, auto_install: bool) {
        if self.update_state.busy() {
            return;
        }
        self.update_log.clear();
        self.settings.updater.last_check_unix = updater::now_unix();
        self.mark_dirty();
        updater::check(
            self.settings.ytdlp_bin(),
            auto_install,
            self.upd_tx.clone(),
            Self::repainter(ctx),
        );
    }

    pub fn start_update_install(&mut self, ctx: &egui::Context) {
        if self.update_state.busy() {
            return;
        }
        updater::install(
            self.settings.ytdlp_bin(),
            self.upd_tx.clone(),
            Self::repainter(ctx),
        );
    }

    fn maybe_auto_check_updates(&mut self, ctx: &egui::Context) {
        let Some(interval) = self.settings.updater.check.interval_secs() else {
            // Even with checks off, read the local version so the UI can show it.
            let bin = self.settings.ytdlp_bin();
            let tx = self.upd_tx.clone();
            let repaint = Self::repainter(ctx);
            std::thread::spawn(move || {
                match updater::current_version(&bin) {
                    Ok(v) => {
                        let _ = tx.send(UpdateEvent::Current(v));
                    }
                    Err(e) => {
                        let _ = tx.send(UpdateEvent::State(UpdateState::Missing(e)));
                    }
                }
                repaint();
            });
            return;
        };

        let due =
            updater::now_unix().saturating_sub(self.settings.updater.last_check_unix) >= interval;
        if due {
            let auto = self.settings.updater.auto_install;
            self.start_update_check(ctx, auto);
            if self.settings.updater.check_app {
                self.start_app_update_check(ctx);
            }
        }
    }

    // --- first-run setup ---------------------------------------------------

    /// Probe for yt-dlp and ffmpeg off the UI thread.
    pub fn start_detection(&mut self, ctx: &egui::Context) {
        self.setup.detecting = true;
        let configured_ytdlp = self.settings.advanced.ytdlp_path.clone();
        let configured_ffmpeg = self.settings.advanced.ffmpeg_path.clone();
        let tx = self.setup_tx.clone();
        let repaint = Self::repainter(ctx);
        std::thread::spawn(move || {
            let ytdlp = crate::installer::detect(&configured_ytdlp);
            let ffmpeg = crate::installer::detect_ffmpeg(&configured_ffmpeg);
            let _ = tx.send(SetupEvent::Detected { ytdlp, ffmpeg });
            repaint();
        });
    }

    /// Download yt-dlp from its own GitHub releases into the config directory.
    pub fn start_ytdlp_install(&mut self, ctx: &egui::Context) {
        if self.setup.install.busy() {
            return;
        }
        // The binary follows the config directory, so honour a pending choice.
        let dir = self.setup.config_dir.join("bin");
        self.setup.log.clear();
        self.setup.error = None;

        let (tx, repaint) = (self.setup_tx.clone(), Self::repainter(ctx));
        let (itx, irx) = channel::<InstallEvent>();
        std::thread::spawn(move || {
            while let Ok(ev) = irx.recv() {
                let _ = tx.send(SetupEvent::Install(ev));
            }
        });
        crate::installer::install(dir, itx, repaint);
    }

    /// Install ffmpeg through the platform's package manager. Only available
    /// where that can be done without asking for root.
    #[cfg(windows)]
    pub fn start_ffmpeg_install(&mut self, ctx: &egui::Context) {
        if self.setup.ffmpeg_install.busy() {
            return;
        }
        self.setup.error = None;

        let (tx, repaint) = (self.setup_tx.clone(), Self::repainter(ctx));
        let (itx, irx) = channel::<InstallEvent>();
        std::thread::spawn(move || {
            while let Ok(ev) = irx.recv() {
                let _ = tx.send(SetupEvent::FfmpegInstall(ev));
            }
        });
        crate::installer::install_ffmpeg(itx, repaint);
    }

    fn drain_setup_events(&mut self) {
        while let Ok(ev) = self.setup_rx.try_recv() {
            match ev {
                SetupEvent::Detected { ytdlp, ffmpeg } => {
                    self.setup.detecting = false;
                    // Share the find with the rest of the app, so the status bar
                    // and updates screen are right the moment setup ends.
                    if let Some((_, version)) = &ytdlp {
                        self.ytdlp_version = version.clone();
                    }
                    self.setup.found_ytdlp = ytdlp;
                    self.setup.found_ffmpeg = ffmpeg;
                }
                SetupEvent::Install(InstallEvent::Log(l)) => self.setup.log.push(l),
                SetupEvent::FfmpegInstall(InstallEvent::Log(l)) => self.setup.log.push(l),
                SetupEvent::FfmpegInstall(InstallEvent::State(st)) => {
                    if let InstallState::Failed(e) = &st {
                        let short: String = e.chars().take(90).collect();
                        self.toast(short, true);
                    }
                    if let InstallState::Done { version, .. } = &st {
                        self.toast("ffmpeg installed", false);
                        self.setup.found_ffmpeg = Some(version.clone());
                    }
                    self.setup.ffmpeg_install = st;
                }
                SetupEvent::Install(InstallEvent::State(st)) => {
                    if let InstallState::Failed(e) = &st {
                        let short: String = e.chars().take(90).collect();
                        self.toast(short, true);
                    }
                    if let InstallState::Done { version, .. } = &st {
                        self.toast(format!("installed yt-dlp {version}"), false);
                        self.ytdlp_version = version.clone();
                    }
                    self.setup.install = st;
                }
            }
        }
    }

    /// Apply the setup choices and move on to the app proper.
    pub fn finish_setup(&mut self, ctx: &egui::Context) {
        // The config directory has to move first: everything else is saved into
        // it. A portable copy has no say in this: it is always beside the exe.
        let chosen = self.setup.config_dir.clone();
        if !crate::bootstrap::is_portable() && chosen != crate::bootstrap::config_dir() {
            if let Err(e) = crate::bootstrap::set_config_dir(&chosen) {
                self.setup.error = Some(e);
                return;
            }
        }

        self.settings.processing.download_dir = self.setup.download_dir.clone();
        if let Err(e) = std::fs::create_dir_all(&self.settings.processing.download_dir) {
            self.setup.error = Some(format!("could not create the download folder: {e}"));
            return;
        }

        // Only pin an explicit path for a copy we installed ourselves; one found
        // on PATH should keep resolving through PATH.
        if let InstallState::Done { path, .. } = &self.setup.install {
            self.settings.advanced.ytdlp_path = path.display().to_string();
        }

        self.settings.setup_done = true;
        match self.settings.save() {
            Ok(()) => {
                self.setup.error = None;
                self.view = View::Home;
                self.toast("all set", false);
                self.maybe_auto_check_updates(ctx);
            }
            Err(e) => self.setup.error = Some(format!("could not save settings: {e}")),
        }
    }

    pub fn start_remux(&mut self, ctx: &egui::Context) {
        let Some(input) = self.remux.input.clone() else {
            self.toast("pick a file to remux first", true);
            return;
        };
        if self.remux.state == RemuxState::Running {
            return;
        }

        let audio_ext = match self.remux.audio_codec.as_str() {
            "copy" => input
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "m4a".into()),
            "aac" => "m4a".into(),
            "libmp3lame" => "mp3".into(),
            "libopus" => "opus".into(),
            "flac" => "flac".into(),
            other => other.to_string(),
        };
        // A stream copy into a bare container is unreliable; default to m4a.
        let audio_ext = if audio_ext == "mp4" || audio_ext == "mkv" || audio_ext == "webm" {
            "m4a".to_string()
        } else {
            audio_ext
        };

        let output =
            crate::remux::output_path(&input, self.remux.op, &self.remux.container, &audio_ext);

        self.remux.log.clear();
        self.remux.progress = 0.0;
        self.remux.cancel_flag = Arc::new(AtomicBool::new(false));
        self.remux.state = RemuxState::Running;

        crate::remux::spawn(
            input,
            output,
            self.remux.op,
            self.remux.audio_codec.clone(),
            self.remux.gif_fps,
            self.remux.gif_width,
            self.settings.clone(),
            self.remux.child.clone(),
            self.remux.cancel_flag.clone(),
            self.remux_tx.clone(),
            Self::repainter(ctx),
        );
    }

    pub fn cancel_remux(&mut self) {
        self.remux
            .cancel_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut g) = self.remux.child.lock() {
            if let Some(c) = g.as_mut() {
                let _ = c.kill();
            }
        }
    }

    fn autosave(&mut self) {
        if self.view == View::Setup {
            return;
        }
        if self.settings != self.saved_snapshot && self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
        if let Some(t) = self.dirty_since {
            if t.elapsed().as_millis() > 600 {
                match self.settings.save() {
                    Ok(()) => self.saved_snapshot = self.settings.clone(),
                    Err(e) => self.toast(format!("could not save settings: {e}"), true),
                }
                self.dirty_since = None;
            }
        }
    }

    fn sync_theme(&mut self, ctx: &egui::Context) {
        if self.applied_appearance != self.settings.appearance {
            self.palette = theme::palette(
                self.settings.appearance.theme,
                self.settings.appearance.accent,
            );
            theme::apply(ctx, &self.palette, self.settings.appearance.ui_scale);
            self.applied_appearance = self.settings.appearance.clone();
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            self.remux.input = Some(path);
            self.remux.state = RemuxState::Idle;
            self.view = View::Remux;
            self.toast("file loaded into remux", false);
        }
    }
}

impl eframe::App for SnagApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_theme(ctx);
        self.drain_job_events();
        self.drain_remux_events();
        self.drain_update_events();
        self.drain_app_update_events();
        self.drain_setup_events();
        self.pump_queue(ctx);
        self.pump_probe(ctx);
        self.handle_dropped_files(ctx);
        self.autosave();

        // Keep the clock ticking while anything is in flight, for speed/ETA text.
        if self.active_job_count() > 0 || self.remux.state == RemuxState::Running {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        // The setup screen owns the whole window: no nav, no status bar.
        if self.view != View::Setup {
            ui::sidebar(self, ctx);
            ui::status_bar(self, ctx);
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(self.palette.bg)
                    .inner_margin(egui::Margin::symmetric(28.0, 22.0)),
            )
            .show(ctx, |ui| match self.view {
                View::Setup => ui::setup::view(self, ui),
                View::Home => ui::home::view(self, ui),
                View::History => ui::history_view::view(self, ui),
                View::Queue => ui::queue::view(self, ui),
                View::Remux => ui::remux_view::view(self, ui),
                View::Settings => ui::settings_view::view(self, ui),
                View::Updates => ui::updates_view::view(self, ui),
                View::About => ui::about::view(self, ui),
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for job in self.jobs.iter_mut() {
            if job.state.is_active() {
                job.cancel();
            }
        }
        self.cancel_remux();
        // Quitting mid-setup should leave no settings behind, so the next launch
        // starts the same first-run flow rather than a half-configured one.
        if self.view != View::Setup {
            let _ = self.settings.save();
        }
    }
}

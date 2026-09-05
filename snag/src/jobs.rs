//! The download queue: one OS thread per running job, talking back to the UI
//! over a channel. Jobs are killable and never block the render loop.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::settings::{Mode, Settings};
use crate::util;
use crate::ytdlp;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobState {
    Queued,
    Starting,
    Downloading,
    PostProcessing,
    Done,
    Failed(String),
    Cancelled,
}

impl JobState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobState::Done | JobState::Failed(_) | JobState::Cancelled
        )
    }
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            JobState::Starting | JobState::Downloading | JobState::PostProcessing
        )
    }
    pub fn label(&self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Starting => "starting",
            JobState::Downloading => "downloading",
            JobState::PostProcessing => "processing",
            JobState::Done => "done",
            JobState::Failed(_) => "failed",
            JobState::Cancelled => "cancelled",
        }
    }
}

/// A message from a worker thread to the UI.
#[derive(Debug)]
pub enum JobEvent {
    Title(u64, String),
    Progress {
        id: u64,
        downloaded: Option<f64>,
        total: Option<f64>,
        speed: Option<f64>,
        eta: Option<f64>,
    },
    State(u64, JobState),
    Log(u64, String),
    File(u64, PathBuf),
}

/// A queue entry, owned by the UI thread.
pub struct Job {
    pub id: u64,
    pub url: String,
    pub mode: Mode,
    pub title: String,
    pub state: JobState,
    pub downloaded: f64,
    pub total: f64,
    pub speed: f64,
    pub eta: f64,
    pub file: Option<PathBuf>,
    pub log: Vec<String>,
    pub show_log: bool,
    /// Shared with the worker so cancel can kill the child process.
    pub child: Arc<Mutex<Option<Child>>>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl Job {
    pub fn new(url: String, mode: Mode) -> Self {
        Self {
            id: next_id(),
            url,
            mode,
            title: String::new(),
            state: JobState::Queued,
            downloaded: 0.0,
            total: 0.0,
            speed: 0.0,
            eta: -1.0,
            file: None,
            log: Vec::new(),
            show_log: false,
            child: Arc::new(Mutex::new(None)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn fraction(&self) -> f32 {
        if self.total > 0.0 {
            (self.downloaded / self.total).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }

    pub fn display_name(&self) -> String {
        if self.title.is_empty() {
            self.url.clone()
        } else {
            self.title.clone()
        }
    }

    /// Ask the worker to stop and kill the yt-dlp process behind it.
    pub fn cancel(&mut self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.child.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
            }
        }
    }
}

/// Spawn the worker thread for a job. Returns immediately.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    id: u64,
    url: String,
    mode: Mode,
    settings: Settings,
    child_slot: Arc<Mutex<Option<Child>>>,
    cancel_flag: Arc<AtomicBool>,
    tx: Sender<JobEvent>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = tx.send(JobEvent::State(id, JobState::Starting));
        repaint();

        let bin = settings.ytdlp_bin();
        let args = ytdlp::build_args(&url, mode, &settings);

        let spawned = util::command(&bin)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn();

        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(JobEvent::State(
                    id,
                    JobState::Failed(format!(
                        "could not start {bin}: {e}. set the yt-dlp path in settings > advanced."
                    )),
                ));
                repaint();
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Ok(mut slot) = child_slot.lock() {
            *slot = Some(child);
        }

        // stderr is drained on its own thread so a full pipe can never deadlock us.
        let err_tx = tx.clone();
        let err_handle = stderr.map(|se| {
            std::thread::spawn(move || {
                let mut collected = Vec::new();
                for line in BufReader::new(se).lines().map_while(Result::ok) {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed.starts_with("ERROR") || collected.len() < 40 {
                        collected.push(trimmed.clone());
                    }
                    let _ = err_tx.send(JobEvent::Log(id, trimmed));
                }
                collected
            })
        });

        let mut announced_title = false;
        let mut last_state = JobState::Starting;

        if let Some(so) = stdout {
            for line in BufReader::new(so).lines().map_while(Result::ok) {
                if cancel_flag.load(Ordering::Relaxed) {
                    break;
                }
                match ytdlp::parse_line(&line) {
                    ytdlp::Line::Progress(p) => {
                        if !announced_title {
                            if let Some(t) = &p.title {
                                let _ = tx.send(JobEvent::Title(id, t.clone()));
                                announced_title = true;
                            }
                        }
                        if last_state != JobState::Downloading {
                            last_state = JobState::Downloading;
                            let _ = tx.send(JobEvent::State(id, JobState::Downloading));
                        }
                        // yt-dlp reports `finished` once per stream, so pin the
                        // bar to full rather than leaving it a hair short.
                        let finished = p.status == "finished";
                        let _ = tx.send(JobEvent::Progress {
                            id,
                            downloaded: if finished {
                                p.total.or(p.downloaded)
                            } else {
                                p.downloaded
                            },
                            total: p.total,
                            speed: if finished { Some(0.0) } else { p.speed },
                            eta: if finished { Some(-1.0) } else { p.eta },
                        });
                        repaint();
                    }
                    ytdlp::Line::PostProcess { status } => {
                        if last_state != JobState::PostProcessing {
                            last_state = JobState::PostProcessing;
                            let _ = tx.send(JobEvent::State(id, JobState::PostProcessing));
                            repaint();
                        }
                        let _ = tx.send(JobEvent::Log(id, format!("postprocess: {status}")));
                    }
                    ytdlp::Line::File(path) => {
                        if !path.is_empty() && path != "NA" {
                            let _ = tx.send(JobEvent::File(id, PathBuf::from(path)));
                        }
                    }
                    ytdlp::Line::Other(text) => {
                        let t = text.trim();
                        if !t.is_empty() {
                            let _ = tx.send(JobEvent::Log(id, t.to_string()));
                        }
                    }
                }
            }
        }

        let status = {
            let mut guard = child_slot.lock().ok();
            match guard.as_mut().and_then(|g| g.as_mut()) {
                Some(c) => c.wait().ok(),
                None => None,
            }
        };
        if let Ok(mut slot) = child_slot.lock() {
            *slot = None;
        }

        let stderr_tail = err_handle.and_then(|h| h.join().ok()).unwrap_or_default();

        let final_state = if cancel_flag.load(Ordering::Relaxed) {
            JobState::Cancelled
        } else if status.map(|s| s.success()).unwrap_or(false) {
            JobState::Done
        } else {
            let reason = stderr_tail
                .iter()
                .rev()
                .find(|l| l.starts_with("ERROR"))
                .cloned()
                .or_else(|| stderr_tail.last().cloned())
                .unwrap_or_else(|| "yt-dlp exited with an error".to_string());
            JobState::Failed(reason)
        };

        let _ = tx.send(JobEvent::State(id, final_state));
        repaint();
    });
}

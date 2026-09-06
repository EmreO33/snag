use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Declares a small closed-set enum with display labels and an ordered list,
/// which is what every pill row and dropdown in the settings UI is built from.
macro_rules! choice_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($variant:ident => $label:expr),+ $(,)? }
        default: $default:ident
    ) => {
        $(#[$meta])*
        #[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
            pub fn label(&self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }
        }

        impl Default for $name {
            fn default() -> Self { $name::$default }
        }
    };
}

choice_enum! {
    /// What Snag pulls out of a link: everything, audio only, or video with no sound.
    Mode {
        Auto => "auto",
        Audio => "audio",
        Mute => "mute",
    }
    default: Auto
}

impl Mode {
    pub fn hint(&self) -> &'static str {
        match self {
            Mode::Auto => "video + audio, merged into one file",
            Mode::Audio => "audio track only",
            Mode::Mute => "video only, audio track dropped",
        }
    }
}

choice_enum! {
    VideoQuality {
        Max => "8k+",
        Q2160 => "4k",
        Q1440 => "1440p",
        Q1080 => "1080p",
        Q720 => "720p",
        Q480 => "480p",
        Q360 => "360p",
        Q240 => "240p",
        Q144 => "144p",
    }
    default: Q1080
}

impl VideoQuality {
    /// Pixel height cap, or None for "take the best there is".
    pub fn height(&self) -> Option<u32> {
        match self {
            VideoQuality::Max => None,
            VideoQuality::Q2160 => Some(2160),
            VideoQuality::Q1440 => Some(1440),
            VideoQuality::Q1080 => Some(1080),
            VideoQuality::Q720 => Some(720),
            VideoQuality::Q480 => Some(480),
            VideoQuality::Q360 => Some(360),
            VideoQuality::Q240 => Some(240),
            VideoQuality::Q144 => Some(144),
        }
    }
}

choice_enum! {
    VideoCodec {
        H264 => "h264 + aac",
        Av1 => "av1 + opus",
        Vp9 => "vp9 + opus",
    }
    default: H264
}

impl VideoCodec {
    /// yt-dlp `--format-sort` codec token.
    pub fn sort_token(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "vcodec:h264",
            VideoCodec::Av1 => "vcodec:av01",
            VideoCodec::Vp9 => "vcodec:vp9",
        }
    }
    pub fn audio_sort_token(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "acodec:aac",
            _ => "acodec:opus",
        }
    }
    pub fn natural_container(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "mp4",
            _ => "webm",
        }
    }
    pub fn note(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "best compatibility, average quality. max quality is 1080p.",
            VideoCodec::Av1 => "best quality and efficiency. supports 8k & HDR.",
            VideoCodec::Vp9 => "same quality as av1, but file is ~2x bigger. supports 4k & HDR.",
        }
    }
}

choice_enum! {
    Container {
        Auto => "auto",
        Mp4 => "mp4",
        Webm => "webm",
        Mkv => "mkv",
    }
    default: Auto
}

impl Container {
    /// Resolve `auto` against the codec the user picked.
    pub fn resolve(&self, codec: VideoCodec) -> &'static str {
        match self {
            Container::Auto => codec.natural_container(),
            Container::Mp4 => "mp4",
            Container::Webm => "webm",
            Container::Mkv => "mkv",
        }
    }
}

choice_enum! {
    AudioFormat {
        Best => "best",
        Mp3 => "mp3",
        Ogg => "ogg",
        Wav => "wav",
        Opus => "opus",
    }
    default: Mp3
}

impl AudioFormat {
    /// The value passed to `--audio-format`.
    pub fn ytdlp_value(&self) -> &'static str {
        match self {
            AudioFormat::Best => "best",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Ogg => "vorbis",
            AudioFormat::Wav => "wav",
            AudioFormat::Opus => "opus",
        }
    }
    /// True when the format is re-encoded, so bitrate actually applies.
    pub fn is_lossy(&self) -> bool {
        matches!(
            self,
            AudioFormat::Mp3 | AudioFormat::Ogg | AudioFormat::Opus
        )
    }
}

choice_enum! {
    AudioBitrate {
        K320 => "320kb/s",
        K256 => "256kb/s",
        K128 => "128kb/s",
        K96 => "96kb/s",
        K64 => "64kb/s",
        K8 => "8kb/s",
    }
    default: K128
}

impl AudioBitrate {
    pub fn kbps(&self) -> u32 {
        match self {
            AudioBitrate::K320 => 320,
            AudioBitrate::K256 => 256,
            AudioBitrate::K128 => 128,
            AudioBitrate::K96 => 96,
            AudioBitrate::K64 => 64,
            AudioBitrate::K8 => 8,
        }
    }
}

choice_enum! {
    UpdateCheck {
        Never => "never",
        OnLaunch => "on launch",
        Daily => "daily",
        Weekly => "weekly",
    }
    default: OnLaunch
}

impl UpdateCheck {
    /// Seconds that must pass before another check is due, or None to never check.
    pub fn interval_secs(&self) -> Option<u64> {
        match self {
            UpdateCheck::Never => None,
            UpdateCheck::OnLaunch => Some(0),
            UpdateCheck::Daily => Some(60 * 60 * 24),
            UpdateCheck::Weekly => Some(60 * 60 * 24 * 7),
        }
    }
}

choice_enum! {
    Accent {
        Mono => "mono",
        Blue => "blue",
        Purple => "purple",
        Green => "green",
        Orange => "orange",
        Pink => "pink",
    }
    default: Mono
}

choice_enum! {
    ThemeMode {
        Dark => "dark",
        Dim => "dim",
        Light => "light",
    }
    default: Dark
}

choice_enum! {
    CookieBrowser {
        None => "none",
        Chrome => "chrome",
        Edge => "edge",
        Firefox => "firefox",
        Brave => "brave",
        Opera => "opera",
        Vivaldi => "vivaldi",
    }
    default: None
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Appearance {
    pub theme: ThemeMode,
    pub accent: Accent,
    pub ui_scale: f32,
    pub compact_queue: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            accent: Accent::Mono,
            ui_scale: 1.0,
            compact_queue: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct VideoSettings {
    pub quality: VideoQuality,
    pub codec: VideoCodec,
    pub container: Container,
    pub allow_h265: bool,
    pub prefer_free_formats: bool,
    pub max_fps: u32,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            quality: VideoQuality::Q1080,
            codec: VideoCodec::H264,
            container: Container::Auto,
            allow_h265: false,
            prefer_free_formats: false,
            max_fps: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct AudioSettings {
    pub format: AudioFormat,
    pub bitrate: AudioBitrate,
    pub prefer_better_quality: bool,
    pub dub_language: String,
    pub normalize_loudness: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            format: AudioFormat::Mp3,
            bitrate: AudioBitrate::K128,
            prefer_better_quality: false,
            dub_language: "original".into(),
            normalize_loudness: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct MetadataSettings {
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub embed_chapters: bool,
    pub embed_subtitles: bool,
    pub subtitle_languages: String,
    pub write_thumbnail_file: bool,
    pub sponsorblock_remove: bool,
    pub keep_original_date: bool,
}

impl Default for MetadataSettings {
    fn default() -> Self {
        Self {
            embed_metadata: true,
            embed_thumbnail: true,
            embed_chapters: false,
            embed_subtitles: false,
            subtitle_languages: "en".into(),
            write_thumbnail_file: false,
            sponsorblock_remove: false,
            keep_original_date: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ProcessingSettings {
    pub download_dir: PathBuf,
    pub output_template: String,
    pub restrict_filenames: bool,
    pub overwrite_existing: bool,
    pub keep_source_after_remux: bool,
    pub max_concurrent_jobs: usize,
    pub concurrent_fragments: u32,
}

impl Default for ProcessingSettings {
    fn default() -> Self {
        Self {
            download_dir: crate::util::default_download_dir(),
            output_template: "%(title)s.%(ext)s".into(),
            restrict_filenames: false,
            overwrite_existing: false,
            keep_source_after_remux: true,
            max_concurrent_jobs: 2,
            concurrent_fragments: 4,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct NetworkSettings {
    pub proxy: String,
    pub rate_limit: String,
    pub retries: u32,
    pub socket_timeout: u32,
    pub cookies_from_browser: CookieBrowser,
    pub cookie_file: String,
    pub user_agent: String,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            proxy: String::new(),
            rate_limit: String::new(),
            retries: 10,
            socket_timeout: 20,
            cookies_from_browser: CookieBrowser::None,
            cookie_file: String::new(),
            user_agent: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct UpdaterSettings {
    pub check: UpdateCheck,
    pub auto_install: bool,
    pub last_check_unix: u64,
    /// Whether the same schedule also looks for a newer Snag.
    pub check_app: bool,
}

impl Default for UpdaterSettings {
    fn default() -> Self {
        Self {
            check: UpdateCheck::OnLaunch,
            auto_install: false,
            last_check_unix: 0,
            check_app: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct AdvancedSettings {
    pub ytdlp_path: String,
    pub ffmpeg_path: String,
    pub extra_args: String,
    pub verbose_log: bool,
    pub ignore_playlists: bool,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            ytdlp_path: String::new(),
            ffmpeg_path: String::new(),
            extra_args: String::new(),
            verbose_log: false,
            ignore_playlists: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Settings {
    /// False until the first-run setup has been completed once.
    pub setup_done: bool,
    pub appearance: Appearance,
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub metadata: MetadataSettings,
    pub processing: ProcessingSettings,
    pub network: NetworkSettings,
    pub updater: UpdaterSettings,
    pub advanced: AdvancedSettings,
}

impl Settings {
    pub fn config_dir() -> PathBuf {
        crate::bootstrap::config_dir()
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("settings.json")
    }

    pub fn load() -> Self {
        match std::fs::read_to_string(Self::config_path()) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        std::fs::create_dir_all(Self::config_dir()).map_err(|e| e.to_string())?;
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(Self::config_path(), raw).map_err(|e| e.to_string())
    }

    /// Path to the yt-dlp binary: the override if set, otherwise whatever is on PATH.
    pub fn ytdlp_bin(&self) -> String {
        let p = self.advanced.ytdlp_path.trim();
        if p.is_empty() {
            "yt-dlp".into()
        } else {
            p.to_string()
        }
    }

    pub fn ffmpeg_bin(&self) -> String {
        let p = self.advanced.ffmpeg_path.trim();
        if p.is_empty() {
            "ffmpeg".into()
        } else {
            p.to_string()
        }
    }
}

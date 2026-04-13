use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

const FFMPEG_POLL_INTERVAL_MS: u64 = 200;
const FFMPEG_TIMEOUT_IMAGE_SECS: u64 = 30;
const FFMPEG_TIMEOUT_METADATA_SECS: u64 = 20;
const FFMPEG_TIMEOUT_TRANSCODE_SECS: u64 = 120;
const FFMPEG_TIMEOUT_OVERLAY_SECS: u64 = 180;

#[derive(Debug, Clone)]
pub struct MediaProbe {
    pub width: u32,
    pub height: u32,
    pub rotation: i32,
    pub display_width: u32,
    pub display_height: u32,
    pub duration_secs: f64,
    pub has_audio: bool,
}

#[derive(Debug, Clone)]
pub struct ImageProbe {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Copy)]
pub enum VideoOutputProfile {
    Mp4Compatible,
    LinuxWebm,
    MovFast,
    MovHighQuality,
}

impl VideoOutputProfile {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("linux_webm") => Self::LinuxWebm,
            Some("mov_fast") => Self::MovFast,
            Some("mov_high_quality") => Self::MovHighQuality,
            Some("auto") => Self::from_setting(Some(&probe_system_codecs().recommended_profile)),
            Some("mp4_compatible") | None | Some(_) => Self::Mp4Compatible,
        }
    }

    pub fn output_extension(self) -> &'static str {
        match self {
            Self::Mp4Compatible => "mp4",
            Self::LinuxWebm => "webm",
            Self::MovFast | Self::MovHighQuality => "mov",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ImageOutputFormat {
    Jpg,
    Webp,
    Png,
}

impl ImageOutputFormat {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("webp") => Self::Webp,
            Some("png") => Self::Png,
            Some("jpg") | None | Some(_) => Self::Jpg,
        }
    }

    pub fn output_extension(self) -> &'static str {
        match self {
            Self::Jpg => "jpg",
            Self::Webp => "webp",
            Self::Png => "png",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ImageQuality {
    Full,
    Balanced,
    Fast,
}

impl ImageQuality {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("balanced") => Self::Balanced,
            Some("fast") => Self::Fast,
            Some("full") | None | Some(_) => Self::Full,
        }
    }
}

/// Hardware-accelerated video encoder detected on the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HwAccelEncoder {
    Nvenc,
    Qsv,
    Vaapi,
}

impl HwAccelEncoder {
    /// The ffmpeg encoder name for H.264 output.
    pub fn codec_name(self) -> &'static str {
        match self {
            Self::Nvenc => "h264_nvenc",
            Self::Qsv => "h264_qsv",
            Self::Vaapi => "h264_vaapi",
        }
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Nvenc => "nvenc",
            Self::Qsv => "qsv",
            Self::Vaapi => "vaapi",
        }
    }
}

/// User preference for hardware-accelerated encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwAccelPreference {
    Auto,
    Nvenc,
    Qsv,
    Vaapi,
    Disabled,
}

impl HwAccelPreference {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("nvenc") => Self::Nvenc,
            Some("qsv") => Self::Qsv,
            Some("vaapi") => Self::Vaapi,
            Some("disabled") => Self::Disabled,
            Some("auto") | None | Some(_) => Self::Auto,
        }
    }

    /// Resolve to a concrete encoder (or `None` for software).
    pub fn resolve(self) -> Option<HwAccelEncoder> {
        match self {
            Self::Auto => best_hw_encoder(),
            Self::Nvenc => Some(HwAccelEncoder::Nvenc),
            Self::Qsv => Some(HwAccelEncoder::Qsv),
            Self::Vaapi => Some(HwAccelEncoder::Vaapi),
            Self::Disabled => None,
        }
    }
}

/// Overlay composition strategy when video and overlay have different resolutions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayStrategy {
    /// Scale video up to the overlay resolution (default, preserves text quality).
    Upscale,
    /// Scale overlay down to the video resolution with a sharpening filter (faster).
    DownscaleWithSharpen,
}

impl OverlayStrategy {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("downscale_sharpen") => Self::DownscaleWithSharpen,
            Some("upscale") | None | Some(_) => Self::Upscale,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MediaEncodingOptions {
    pub video_profile: VideoOutputProfile,
    pub image_format: ImageOutputFormat,
    pub image_quality: ImageQuality,
    pub hw_accel: HwAccelPreference,
    pub overlay_strategy: OverlayStrategy,
}

#[derive(Debug)]
pub enum MediaError {
    Io(std::io::Error),
    Join(tokio::task::JoinError),
    UnsupportedMediaType(PathBuf),
    MissingOverlay(PathBuf),
    InvalidMetadata(String),
    FfmpegFailed { status: Option<i32>, stderr: String },
}

impl Display for MediaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "media processing I/O failed: {error}"),
            Self::Join(error) => write!(f, "media processing thread failed: {error}"),
            Self::UnsupportedMediaType(path) => {
                write!(
                    f,
                    "unsupported media type for '{}': expected image (jpg/jpeg/png/webp) or video (mp4/mov/m4v/webm)",
                    path.display()
                )
            }
            Self::MissingOverlay(path) => {
                write!(f, "overlay file does not exist at '{}'", path.display())
            }
            Self::InvalidMetadata(reason) => write!(f, "invalid media metadata: {reason}"),
            Self::FfmpegFailed { status, stderr } => {
                write!(
                    f,
                    "ffmpeg exited with status {:?}: {}",
                    status,
                    stderr.trim()
                )
            }
        }
    }
}

impl std::error::Error for MediaError {}

impl From<std::io::Error> for MediaError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<tokio::task::JoinError> for MediaError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Join(value)
    }
}

pub async fn merge_media_with_optional_overlay(
    base_media_path: &Path,
    overlay_path: Option<&Path>,
    output_path: &Path,
    encoding_options: MediaEncodingOptions,
    video_probe: Option<&MediaProbe>,
    overlay_probe: Option<&ImageProbe>,
) -> Result<(), MediaError> {
    let base_media_path = base_media_path.to_path_buf();
    let overlay_path = overlay_path.map(Path::to_path_buf);
    let output_path = output_path.to_path_buf();
    let video_probe = video_probe.cloned();
    let overlay_probe = overlay_probe.cloned();

    tokio::task::spawn_blocking(move || {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match overlay_path {
            Some(overlay_path) => {
                if !overlay_path.exists() {
                    return Err(MediaError::MissingOverlay(overlay_path));
                }

                let media_kind = media_kind_from_path(&base_media_path)
                    .ok_or_else(|| MediaError::UnsupportedMediaType(base_media_path.clone()))?;

                let ffmpeg_output_path =
                    temporary_output_path_with_suffix(&output_path, "overlay.tmp")?;

                let args = build_ffmpeg_overlay_args(
                    &base_media_path,
                    &overlay_path,
                    &ffmpeg_output_path,
                    media_kind,
                    encoding_options,
                    video_probe.as_ref(),
                    overlay_probe.as_ref(),
                );

                let timeout = match media_kind {
                    MediaKind::Video => {
                        let base = FFMPEG_TIMEOUT_OVERLAY_SECS;
                        if let Some(probe) = &video_probe {
                            base.max((probe.duration_secs * 20.0) as u64)
                        } else {
                            base
                        }
                    }
                    MediaKind::Image => FFMPEG_TIMEOUT_IMAGE_SECS,
                };

                run_ffmpeg(args, timeout)?;
                std::fs::rename(&ffmpeg_output_path, &output_path)?;

                Ok(())
            }
            None => {
                let media_kind = media_kind_from_path(&base_media_path)
                    .ok_or_else(|| MediaError::UnsupportedMediaType(base_media_path.clone()))?;

                match media_kind {
                    MediaKind::Image => {
                        let ffmpeg_output_path =
                            temporary_output_path_with_suffix(&output_path, "image.tmp")?;
                        let args = build_ffmpeg_image_transcode_args(
                            &base_media_path,
                            &ffmpeg_output_path,
                            encoding_options,
                        );
                        run_ffmpeg(args, FFMPEG_TIMEOUT_IMAGE_SECS)?;
                        std::fs::rename(&ffmpeg_output_path, &output_path)?;
                        Ok(())
                    }
                    MediaKind::Video => {
                        let ffmpeg_output_path =
                            temporary_output_path_with_suffix(&output_path, "normalize.tmp")?;

                        let args = build_ffmpeg_video_normalize_args(
                            &base_media_path,
                            &ffmpeg_output_path,
                            encoding_options,
                        );
                        run_ffmpeg(args, FFMPEG_TIMEOUT_TRANSCODE_SECS)?;
                        std::fs::rename(&ffmpeg_output_path, &output_path)?;

                        Ok(())
                    }
                }
            }
        }
    })
    .await??;

    Ok(())
}

pub async fn write_metadata_with_ffmpeg(
    media_path: &Path,
    date_taken: &str,
    location: Option<&str>,
) -> Result<(), MediaError> {
    let media_path = media_path.to_path_buf();
    let date_taken = normalize_datetime_for_ffmpeg(date_taken)?;
    let coordinates = location.and_then(parse_coordinates);

    tokio::task::spawn_blocking(move || {
        let media_kind = media_kind_from_path(&media_path)
            .ok_or_else(|| MediaError::UnsupportedMediaType(media_path.clone()))?;

        let temp_output_path = temporary_output_path(&media_path)?;
        let args = build_ffmpeg_metadata_args(
            &media_path,
            &temp_output_path,
            &date_taken,
            coordinates,
            media_kind,
        );

        run_ffmpeg(args, FFMPEG_TIMEOUT_METADATA_SECS)?;
        std::fs::rename(&temp_output_path, &media_path)?;
        Ok::<(), MediaError>(())
    })
    .await??;

    Ok(())
}

pub async fn cleanup_intermediate_files(
    raw_media_path: &Path,
    overlay_path: Option<&Path>,
    final_media_path: &Path,
) -> Result<(), MediaError> {
    let raw_media_path = raw_media_path.to_path_buf();
    let overlay_path = overlay_path.map(Path::to_path_buf);
    let final_media_path = final_media_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        if raw_media_path != final_media_path {
            remove_file_if_exists(&raw_media_path)?;
        }

        if let Some(overlay_path) = overlay_path {
            if overlay_path != final_media_path {
                remove_file_if_exists(&overlay_path)?;
            }
        }

        Ok::<(), MediaError>(())
    })
    .await??;

    Ok(())
}

pub async fn probe_media(path: &Path) -> Result<MediaProbe, MediaError> {
    let path = path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height,duration,codec_type",
                "-show_entries",
                "stream_side_data=rotation",
                "-show_entries",
                "format=duration",
                "-of",
                "default=nokey=0:noprint_wrappers=1",
            ])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(MediaError::Io)?;

        if !output.status.success() {
            return Err(MediaError::FfmpegFailed {
                status: output.status.code(),
                stderr: format!(
                    "ffprobe failed for '{}': {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        let mut rotation: i32 = 0;
        let mut duration_secs: f64 = 0.0;

        for line in stdout.lines() {
            if let Some(v) = line.strip_prefix("width=") {
                width = v.trim().parse().unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("height=") {
                height = v.trim().parse().unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("rotation=") {
                rotation = v.trim().parse().unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("duration=") {
                if duration_secs <= 0.0 {
                    duration_secs = v.trim().parse().unwrap_or(0.0);
                }
            }
        }

        // Check for audio stream presence separately.
        let audio_output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "default=nokey=0:noprint_wrappers=1",
            ])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(MediaError::Io)?;

        let has_audio = String::from_utf8_lossy(&audio_output.stdout)
            .lines()
            .any(|l| l.contains("audio"));

        let (display_width, display_height) = if rotation.abs() == 90 || rotation.abs() == 270 {
            (height, width)
        } else {
            (width, height)
        };

        eprintln!(
            "[media-probe] '{}': {}x{} rotation={} display={}x{} duration={:.2}s audio={}",
            path.display(),
            width,
            height,
            rotation,
            display_width,
            display_height,
            duration_secs,
            has_audio
        );

        Ok(MediaProbe {
            width,
            height,
            rotation,
            display_width,
            display_height,
            duration_secs,
            has_audio,
        })
    })
    .await?
}

pub async fn probe_image(path: &Path) -> Result<ImageProbe, MediaError> {
    let path = path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height",
                "-of",
                "default=nokey=0:noprint_wrappers=1",
            ])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(MediaError::Io)?;

        if !output.status.success() {
            return Err(MediaError::FfmpegFailed {
                status: output.status.code(),
                stderr: format!(
                    "ffprobe failed for '{}': {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut width: u32 = 0;
        let mut height: u32 = 0;

        for line in stdout.lines() {
            if let Some(v) = line.strip_prefix("width=") {
                width = v.trim().parse().unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("height=") {
                height = v.trim().parse().unwrap_or(0);
            }
        }

        eprintln!("[media-probe] '{}': {}x{}", path.display(), width, height);

        Ok(ImageProbe { width, height })
    })
    .await?
}

fn run_ffmpeg(args: Vec<String>, timeout_secs: u64) -> Result<(), MediaError> {
    eprintln!(
        "[ffmpeg] cmd (timeout={}s): ffmpeg {}",
        timeout_secs,
        args.join(" ")
    );

    let stderr_log_path = std::env::temp_dir().join(format!(
        "memorysnaper-ffmpeg-{}-{}.log",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let stderr_log_file = std::fs::File::create(&stderr_log_path).map_err(MediaError::Io)?;
    let stderr_for_child = stderr_log_file.try_clone().map_err(MediaError::Io)?;

    let mut ffmpeg_args = vec![
        "-hide_banner".to_string(),
        "-nostdin".to_string(),
        "-loglevel".to_string(),
        "warning".to_string(),
    ];
    ffmpeg_args.extend(args.iter().cloned());

    let mut child = Command::new("ffmpeg")
        .args(&ffmpeg_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_for_child))
        .spawn()
        .map_err(MediaError::Io)?;

    let started_at = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(MediaError::Io)? {
            break status;
        }

        if started_at.elapsed() >= Duration::from_secs(timeout_secs) {
            timed_out = true;
            let _ = child.kill();
            break child.wait().map_err(MediaError::Io)?;
        }

        std::thread::sleep(Duration::from_millis(FFMPEG_POLL_INTERVAL_MS));
    };

    let stderr_text = std::fs::read_to_string(&stderr_log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&stderr_log_path);

    if timed_out {
        return Err(MediaError::FfmpegFailed {
            status: None,
            stderr: format!(
                "ffmpeg timed out after {}s{}{}",
                timeout_secs,
                if stderr_text.trim().is_empty() {
                    ""
                } else {
                    "; stderr: "
                },
                stderr_text.trim()
            ),
        });
    }

    if status.success() {
        // Log warnings from stderr even on success — encoder warnings about
        // color space mismatches or deprecated options surface here.
        let warnings: Vec<&str> = stderr_text
            .lines()
            .filter(|line| {
                let lower = line.to_ascii_lowercase();
                lower.contains("warning") || lower.contains("discarding")
            })
            .collect();

        if !warnings.is_empty() {
            eprintln!("[ffmpeg] completed with {} warning(s):", warnings.len());
            for warning in warnings {
                eprintln!("[ffmpeg]   {}", warning.trim());
            }
        }

        return Ok(());
    }

    let output_path = args.last().map(String::as_str).unwrap_or("unknown");
    eprintln!(
        "[ffmpeg] FAILED status={:?} output='{}'\n[ffmpeg] stderr:\n{}",
        status.code(),
        output_path,
        stderr_text.trim()
    );

    Err(MediaError::FfmpegFailed {
        status: status.code(),
        stderr: stderr_text,
    })
}

fn media_kind_from_path(path: &Path) -> Option<MediaKind> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();

    match extension.as_str() {
        "jpg" | "jpeg" | "png" | "webp" => Some(MediaKind::Image),
        "mp4" | "mov" | "m4v" | "webm" => Some(MediaKind::Video),
        _ => None,
    }
}

fn build_ffmpeg_overlay_args(
    base_media_path: &Path,
    overlay_path: &Path,
    output_path: &Path,
    media_kind: MediaKind,
    encoding_options: MediaEncodingOptions,
    video_probe: Option<&MediaProbe>,
    overlay_probe: Option<&ImageProbe>,
) -> Vec<String> {
    let hw_encoder = if matches!(media_kind, MediaKind::Video) {
        resolve_hw_encoder(&encoding_options)
    } else {
        None
    };

    // VAAPI: device init must come before any -i inputs.
    let mut args: Vec<String> = if matches!(hw_encoder, Some(HwAccelEncoder::Vaapi)) {
        vaapi_init_args()
    } else {
        Vec::new()
    };

    match media_kind {
        MediaKind::Image => {
            args.push("-y".to_string());
            args.push("-i".to_string());
            args.push(base_media_path.to_string_lossy().to_string());
            args.push("-i".to_string());
            args.push(overlay_path.to_string_lossy().to_string());

            let filter = build_image_overlay_filter(
                video_probe,
                overlay_probe,
                encoding_options.overlay_strategy,
            );
            args.push("-filter_complex".to_string());
            args.push(filter);
            args.push("-map".to_string());
            args.push("[vout]".to_string());
            args.push("-frames:v".to_string());
            args.push("1".to_string());
            append_image_encoding_args(
                &mut args,
                encoding_options.image_format,
                encoding_options.image_quality,
            );
        }
        MediaKind::Video => {
            // Let ffmpeg autorotate handle rotation natively. We use probed
            // display dimensions for scaling, so autorotate won't conflict.
            args.push("-y".to_string());
            args.push("-i".to_string());
            args.push(base_media_path.to_string_lossy().to_string());
            args.push("-i".to_string());
            args.push(overlay_path.to_string_lossy().to_string());
            args.push("-threads".to_string());
            args.push("0".to_string());

            let filter = build_video_overlay_filter(
                video_probe,
                overlay_probe,
                encoding_options.overlay_strategy,
                hw_encoder,
            );
            args.push("-filter_complex".to_string());
            args.push(filter);
            args.push("-map".to_string());
            args.push("[vout]".to_string());
            args.push("-map".to_string());
            args.push("0:a?".to_string());

            // Use explicit duration limit instead of -shortest to prevent
            // hangs when the overlay stream confuses EOF detection.
            if let Some(probe) = video_probe {
                if probe.duration_secs > 0.0 {
                    args.push("-t".to_string());
                    args.push(format!("{:.3}", probe.duration_secs));
                }
            } else {
                args.push("-shortest".to_string());
            }

            append_video_encoding_args(
                &mut args,
                encoding_options.video_profile,
                resolve_hw_encoder(&encoding_options),
            );
        }
    }

    args.push(output_path.to_string_lossy().to_string());
    args
}

/// Build a filter graph for compositing a video with a PNG overlay.
///
/// Strategy depends on `overlay_strategy`:
/// - `Upscale`: target = max(video, overlay) → both scaled up (current default).
/// - `DownscaleWithSharpen`: target = video dims → overlay scaled down + unsharp.
///
/// When `hw_encoder` is VAAPI, appends `format=nv12,hwupload` at the end.
fn build_video_overlay_filter(
    video_probe: Option<&MediaProbe>,
    overlay_probe: Option<&ImageProbe>,
    strategy: OverlayStrategy,
    hw_encoder: Option<HwAccelEncoder>,
) -> String {
    let (vid_w, vid_h, _rotation) = match video_probe {
        Some(p) => (p.display_width, p.display_height, p.rotation),
        None => (0, 0, 0),
    };
    let (ov_w, ov_h) = match overlay_probe {
        Some(p) => (p.width, p.height),
        None => (0, 0),
    };

    let vaapi_suffix = if matches!(hw_encoder, Some(HwAccelEncoder::Vaapi)) {
        ",format=nv12,hwupload"
    } else {
        ""
    };

    // If we have no probe data, fall back to a simple scale2ref approach.
    if vid_w == 0 || vid_h == 0 || ov_w == 0 || ov_h == 0 {
        return format!(
            "[1:v]format=rgba[ovsrc];[ovsrc][0:v]scale2ref=w=main_w:h=main_h[ov][base];\
             [base][ov]overlay=0:0:format=auto,format=yuv420p{vaapi_suffix}[vout]"
        );
    }

    let (target_w, target_h) = match strategy {
        OverlayStrategy::Upscale => {
            let tw = vid_w.max(ov_w);
            let th = vid_h.max(ov_h);
            (tw, th)
        }
        OverlayStrategy::DownscaleWithSharpen => (vid_w, vid_h),
    };
    // Ensure dimensions are even (required by H.264 / yuv420p).
    let target_w = (target_w + 1) & !1;
    let target_h = (target_h + 1) & !1;

    let mut parts: Vec<String> = Vec::new();

    // Step 1: Scale base video to target dimensions.
    // Rotation is handled by ffmpeg autorotate (default ON), so the frames
    // arriving in the filter graph are already in display orientation.
    parts.push(format!(
        "[0:v]scale={target_w}:{target_h}:flags=lanczos[base]"
    ));

    // Step 2: Scale overlay to target dimensions, optionally with sharpening.
    let sharpen_suffix = if matches!(strategy, OverlayStrategy::DownscaleWithSharpen) {
        ",unsharp=5:5:1.0:5:5:0.0"
    } else {
        ""
    };
    parts.push(format!(
        "[1:v]format=rgba,scale={target_w}:{target_h}:flags=lanczos{sharpen_suffix}[ov]"
    ));

    // Step 3: Overlay and output to yuv420p.
    parts.push(format!(
        "[base][ov]overlay=0:0:format=auto,format=yuv420p{vaapi_suffix}[vout]"
    ));

    parts.join(";")
}

/// Build a filter graph for compositing an image with a PNG overlay.
///
/// Scales inputs based on the overlay strategy.
fn build_image_overlay_filter(
    base_probe: Option<&MediaProbe>,
    overlay_probe: Option<&ImageProbe>,
    strategy: OverlayStrategy,
) -> String {
    // For images, MediaProbe width/height are the actual dimensions (no rotation concern).
    let (base_w, base_h) = match base_probe {
        Some(p) => (p.width, p.height),
        None => (0, 0),
    };
    let (ov_w, ov_h) = match overlay_probe {
        Some(p) => (p.width, p.height),
        None => (0, 0),
    };

    // No probe data → simple overlay.
    if base_w == 0 || base_h == 0 || ov_w == 0 || ov_h == 0 {
        return "[0:v][1:v]overlay=0:0:format=auto[vout]".to_string();
    }

    let (target_w, target_h) = match strategy {
        OverlayStrategy::Upscale => (base_w.max(ov_w), base_h.max(ov_h)),
        OverlayStrategy::DownscaleWithSharpen => (base_w, base_h),
    };
    let target_w = (target_w + 1) & !1;
    let target_h = (target_h + 1) & !1;

    let sharpen_suffix = if matches!(strategy, OverlayStrategy::DownscaleWithSharpen) {
        ",unsharp=5:5:1.0:5:5:0.0"
    } else {
        ""
    };

    format!(
        "[0:v]scale={target_w}:{target_h}:flags=lanczos[base];\
         [1:v]format=rgba,scale={target_w}:{target_h}:flags=lanczos{sharpen_suffix}[ov];\
         [base][ov]overlay=0:0:format=auto[vout]"
    )
}

/// Maps rotation degrees to the ffmpeg `transpose` filter value.
///
/// Returns `None` when no rotation is needed (0° or 180° handled by hflip/vflip
/// but 180° is rare for Snapchat content; we handle ±90/270 which is the common case).
#[allow(dead_code)]
fn transpose_value(rotation: i32) -> Option<&'static str> {
    match rotation {
        90 | -270 => Some("1"),              // 90° clockwise
        -90 | 270 => Some("2"),              // 90° counter-clockwise
        180 | -180 => Some("2,transpose=2"), // 180° = two 90° rotations
        _ => None,
    }
}

fn build_ffmpeg_video_normalize_args(
    base_media_path: &Path,
    output_path: &Path,
    encoding_options: MediaEncodingOptions,
) -> Vec<String> {
    let hw_encoder = resolve_hw_encoder(&encoding_options);

    // VAAPI: add device init before inputs.
    let mut args = if matches!(hw_encoder, Some(HwAccelEncoder::Vaapi)) {
        vaapi_init_args()
    } else {
        Vec::new()
    };

    args.extend([
        "-y".to_string(),
        "-i".to_string(),
        base_media_path.to_string_lossy().to_string(),
        "-map".to_string(),
        "0:v".to_string(),
        "-map".to_string(),
        "0:a?".to_string(),
        "-dn".to_string(),
    ]);

    // VAAPI: add upload filter when no overlay filter graph is present.
    if matches!(hw_encoder, Some(HwAccelEncoder::Vaapi)) {
        args.push("-vf".to_string());
        args.push("format=nv12,hwupload".to_string());
    }

    append_video_encoding_args(&mut args, encoding_options.video_profile, hw_encoder);
    args.push(output_path.to_string_lossy().to_string());
    args
}

fn build_ffmpeg_image_transcode_args(
    base_media_path: &Path,
    output_path: &Path,
    encoding_options: MediaEncodingOptions,
) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        base_media_path.to_string_lossy().to_string(),
        "-frames:v".to_string(),
        "1".to_string(),
    ];

    append_image_encoding_args(
        &mut args,
        encoding_options.image_format,
        encoding_options.image_quality,
    );
    args.push(output_path.to_string_lossy().to_string());
    args
}

fn append_video_encoding_args(
    args: &mut Vec<String>,
    profile: VideoOutputProfile,
    hw_encoder: Option<HwAccelEncoder>,
) {
    // VP9/WebM never uses hw acceleration.
    let effective_hw = if matches!(profile, VideoOutputProfile::LinuxWebm) {
        None
    } else {
        hw_encoder
    };

    match profile {
        VideoOutputProfile::Mp4Compatible => {
            append_h264_encoding_args(args, effective_hw, "veryfast", 18, "high", "128k");
            args.push("-movflags".to_string());
            args.push("+faststart".to_string());
        }
        VideoOutputProfile::LinuxWebm => {
            args.push("-c:v".to_string());
            args.push("libvpx-vp9".to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv420p".to_string());
            args.push("-b:v".to_string());
            args.push("0".to_string());
            args.push("-crf".to_string());
            args.push("20".to_string());
            args.push("-g".to_string());
            args.push("240".to_string());
            args.push("-tile-columns".to_string());
            args.push("2".to_string());
            args.push("-tile-rows".to_string());
            args.push("1".to_string());
            args.push("-row-mt".to_string());
            args.push("1".to_string());
            args.push("-deadline".to_string());
            args.push("good".to_string());
            args.push("-cpu-used".to_string());
            args.push("3".to_string());
            args.push("-error-resilient".to_string());
            args.push("1".to_string());
            args.push("-c:a".to_string());
            args.push("libopus".to_string());
            args.push("-b:a".to_string());
            args.push("128k".to_string());
            args.push("-f".to_string());
            args.push("webm".to_string());
        }
        VideoOutputProfile::MovFast => {
            append_h264_encoding_args(args, effective_hw, "ultrafast", 23, "main", "128k");
            args.push("-movflags".to_string());
            args.push("+faststart".to_string());
        }
        VideoOutputProfile::MovHighQuality => {
            append_h264_encoding_args(args, effective_hw, "slow", 16, "high", "192k");
            args.push("-movflags".to_string());
            args.push("+faststart".to_string());
        }
    }

    // Explicit color space metadata for consistent decoding across players.
    // Without these flags, GStreamer (used by WebKitGTK on Linux) may
    // misinterpret the color space, causing wrong colors and visual artifacts.
    args.push("-colorspace".to_string());
    args.push("bt709".to_string());
    args.push("-color_primaries".to_string());
    args.push("bt709".to_string());
    args.push("-color_trc".to_string());
    args.push("bt709".to_string());
    args.push("-color_range".to_string());
    args.push("tv".to_string());

    args.push("-max_muxing_queue_size".to_string());
    args.push("1024".to_string());
}

/// Appends H.264 encoding args, using a HW encoder when available.
fn append_h264_encoding_args(
    args: &mut Vec<String>,
    hw_encoder: Option<HwAccelEncoder>,
    sw_preset: &str,
    quality: u8,
    h264_profile: &str,
    audio_bitrate: &str,
) {
    match hw_encoder {
        Some(HwAccelEncoder::Nvenc) => {
            args.push("-c:v".to_string());
            args.push("h264_nvenc".to_string());
            args.push("-preset".to_string());
            args.push("p4".to_string());
            args.push("-rc".to_string());
            args.push("vbr".to_string());
            args.push("-cq".to_string());
            args.push(quality.to_string());
            args.push("-profile:v".to_string());
            args.push(h264_profile.to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv420p".to_string());
        }
        Some(HwAccelEncoder::Qsv) => {
            args.push("-c:v".to_string());
            args.push("h264_qsv".to_string());
            args.push("-preset".to_string());
            args.push("medium".to_string());
            args.push("-global_quality".to_string());
            args.push(quality.to_string());
            args.push("-pix_fmt".to_string());
            args.push("nv12".to_string());
        }
        Some(HwAccelEncoder::Vaapi) => {
            args.push("-c:v".to_string());
            args.push("h264_vaapi".to_string());
            args.push("-qp".to_string());
            args.push(quality.to_string());
            args.push("-profile:v".to_string());
            args.push(h264_profile.to_string());
        }
        None => {
            args.push("-c:v".to_string());
            args.push("libx264".to_string());
            args.push("-preset".to_string());
            args.push(sw_preset.to_string());
            args.push("-crf".to_string());
            args.push(quality.to_string());
            args.push("-profile:v".to_string());
            args.push(h264_profile.to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv420p".to_string());
        }
    }
    args.push("-c:a".to_string());
    args.push("aac".to_string());
    args.push("-b:a".to_string());
    args.push(audio_bitrate.to_string());
}

fn append_image_encoding_args(
    args: &mut Vec<String>,
    image_format: ImageOutputFormat,
    image_quality: ImageQuality,
) {
    match image_format {
        ImageOutputFormat::Jpg => {
            args.push("-c:v".to_string());
            args.push("mjpeg".to_string());
            args.push("-q:v".to_string());
            args.push(
                match image_quality {
                    ImageQuality::Full => "2",
                    ImageQuality::Balanced => "5",
                    ImageQuality::Fast => "8",
                }
                .to_string(),
            );
        }
        ImageOutputFormat::Webp => {
            args.push("-c:v".to_string());
            args.push("libwebp".to_string());
            args.push("-quality".to_string());
            args.push(
                match image_quality {
                    ImageQuality::Full => "100",
                    ImageQuality::Balanced => "86",
                    ImageQuality::Fast => "72",
                }
                .to_string(),
            );
            args.push("-compression_level".to_string());
            args.push(
                match image_quality {
                    ImageQuality::Full => "6",
                    ImageQuality::Balanced => "4",
                    ImageQuality::Fast => "2",
                }
                .to_string(),
            );
        }
        ImageOutputFormat::Png => {
            args.push("-c:v".to_string());
            args.push("png".to_string());
            args.push("-compression_level".to_string());
            args.push(
                match image_quality {
                    ImageQuality::Full => "9",
                    ImageQuality::Balanced => "6",
                    ImageQuality::Fast => "2",
                }
                .to_string(),
            );
        }
    }
}

fn build_ffmpeg_metadata_args(
    media_path: &Path,
    output_path: &Path,
    date_taken: &str,
    coordinates: Option<(f64, f64)>,
    media_kind: MediaKind,
) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        media_path.to_string_lossy().to_string(),
        "-metadata".to_string(),
        format!("DateTimeOriginal={date_taken}"),
        "-metadata".to_string(),
        format!("DateTimeDigitized={date_taken}"),
        "-metadata".to_string(),
        format!("creation_time={date_taken}"),
    ];

    if let Some((latitude, longitude)) = coordinates {
        args.push("-metadata".to_string());
        args.push(format!("GPSLatitude={latitude}"));
        args.push("-metadata".to_string());
        args.push(format!("GPSLongitude={longitude}"));
        args.push("-metadata".to_string());
        args.push(format!(
            "location={}{}{}{}",
            if latitude >= 0.0 { "+" } else { "" },
            latitude,
            if longitude >= 0.0 { "+" } else { "" },
            longitude
        ));
    }

    match media_kind {
        MediaKind::Image => {
            args.push("-frames:v".to_string());
            args.push("1".to_string());
        }
        MediaKind::Video => {
            args.push("-map".to_string());
            args.push("0:v".to_string());
            args.push("-map".to_string());
            args.push("0:a?".to_string());
            args.push("-dn".to_string());
            args.push("-c".to_string());
            args.push("copy".to_string());
        }
    }

    args.push(output_path.to_string_lossy().to_string());
    args
}

fn normalize_datetime_for_ffmpeg(value: &str) -> Result<String, MediaError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MediaError::InvalidMetadata("date is empty".to_string()));
    }

    if trimmed.len() >= 19 && trimmed.as_bytes().get(10) == Some(&b'T') {
        let date = &trimmed[..10];
        let time = &trimmed[11..19];
        return Ok(format!("{} {}", date.replace('-', ":"), time));
    }

    if trimmed.len() >= 19
        && trimmed.as_bytes().get(4) == Some(&b'-')
        && trimmed.as_bytes().get(7) == Some(&b'-')
        && trimmed.as_bytes().get(10) == Some(&b' ')
    {
        let date = &trimmed[..10];
        let time = &trimmed[11..19];
        return Ok(format!("{} {}", date.replace('-', ":"), time));
    }

    Ok(trimmed.to_string())
}

fn parse_coordinates(value: &str) -> Option<(f64, f64)> {
    let mut parts = value.split(',').map(str::trim);
    let latitude = parts.next()?.parse::<f64>().ok()?;
    let longitude = parts.next()?.parse::<f64>().ok()?;
    Some((latitude, longitude))
}

fn temporary_output_path(media_path: &Path) -> Result<PathBuf, MediaError> {
    temporary_output_path_with_suffix(media_path, "metadata.tmp")
}

fn temporary_output_path_with_suffix(
    media_path: &Path,
    suffix: &str,
) -> Result<PathBuf, MediaError> {
    let stem = media_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            MediaError::InvalidMetadata("media path does not contain a valid file stem".to_string())
        })?;

    let extension = media_path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            MediaError::InvalidMetadata("media path does not contain a valid extension".to_string())
        })?;

    Ok(media_path.with_file_name(format!("{stem}.{suffix}.{extension}")))
}

fn remove_file_if_exists(path: &Path) -> Result<(), MediaError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

/// Probes the system for GStreamer codec availability.
///
/// Returns a [`SystemCodecInfo`] describing which video profiles are likely to
/// play back correctly in WebKitGTK's GStreamer pipeline.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemCodecInfo {
    pub has_h264_decoder: bool,
    pub has_vp9_decoder: bool,
    pub has_opus_decoder: bool,
    pub has_aac_decoder: bool,
    pub recommended_profile: String,
    pub available_hw_encoders: Vec<String>,
    pub recommended_hw_encoder: Option<String>,
}

pub fn probe_system_codecs() -> SystemCodecInfo {
    let has_h264 = gst_element_exists("avdec_h264")
        || gst_element_exists("openh264dec")
        || gst_element_exists("vaapih264dec")
        || gst_element_exists("vah264dec");

    let has_vp9 = gst_element_exists("vp9dec")
        || gst_element_exists("vp9alphadecodebin")
        || gst_element_exists("vaapivp9dec")
        || gst_element_exists("vavp9dec");

    let has_opus = gst_element_exists("opusdec");
    let has_aac = gst_element_exists("avdec_aac") || gst_element_exists("faad");

    let recommended = if has_h264 && has_aac {
        "mp4_compatible"
    } else if has_vp9 && has_opus {
        "linux_webm"
    } else {
        "mp4_compatible"
    };

    SystemCodecInfo {
        has_h264_decoder: has_h264,
        has_vp9_decoder: has_vp9,
        has_opus_decoder: has_opus,
        has_aac_decoder: has_aac,
        recommended_profile: recommended.to_string(),
        available_hw_encoders: probe_hw_encoders()
            .iter()
            .map(|e| e.label().to_string())
            .collect(),
        recommended_hw_encoder: best_hw_encoder().map(|e| e.label().to_string()),
    }
}

fn gst_element_exists(element_name: &str) -> bool {
    Command::new("gst-inspect-1.0")
        .arg(element_name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Cached list of hardware encoders detected via `ffmpeg -encoders`.
static HW_ENCODERS: OnceLock<Vec<HwAccelEncoder>> = OnceLock::new();

/// Probes ffmpeg for available hardware-accelerated H.264 encoders.
/// Results are cached for the lifetime of the process.
pub fn probe_hw_encoders() -> &'static [HwAccelEncoder] {
    HW_ENCODERS.get_or_init(|| {
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        let stdout = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => return Vec::new(),
        };

        parse_hw_encoders_from_output(&stdout)
    })
}

/// Parse the output of `ffmpeg -encoders` to find hardware H.264 encoders.
fn parse_hw_encoders_from_output(output: &str) -> Vec<HwAccelEncoder> {
    // Order matters: NVENC > QSV > VAAPI (priority for `best_hw_encoder`).
    let candidates = [
        ("h264_nvenc", HwAccelEncoder::Nvenc),
        ("h264_qsv", HwAccelEncoder::Qsv),
        ("h264_vaapi", HwAccelEncoder::Vaapi),
    ];

    let mut found = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        for &(name, encoder) in &candidates {
            // Lines look like: " V..... h264_nvenc   NVIDIA NVENC H.264 encoder (codec h264)"
            if trimmed.contains(name) && !found.contains(&encoder) {
                found.push(encoder);
            }
        }
    }
    found
}

/// Returns the best available hardware encoder, or `None` if none is detected.
pub fn best_hw_encoder() -> Option<HwAccelEncoder> {
    probe_hw_encoders().first().copied()
}

/// Resolve the effective hardware encoder for the given encoding options.
/// Returns `None` when software encoding should be used.
fn resolve_hw_encoder(opts: &MediaEncodingOptions) -> Option<HwAccelEncoder> {
    if matches!(opts.video_profile, VideoOutputProfile::LinuxWebm) {
        return None;
    }
    opts.hw_accel.resolve()
}

/// VAAPI init args placed before `-i` to open the render device.
fn vaapi_init_args() -> Vec<String> {
    let device =
        std::env::var("LIBVA_RENDER_DEVICE").unwrap_or_else(|_| "/dev/dri/renderD128".to_string());
    vec!["-vaapi_device".to_string(), device]
}

/// Runs `ffprobe` on the output to verify it has at least one video stream
/// and that its pixel format and codec match expectations.
pub async fn verify_video_integrity(
    video_path: &Path,
    expected_profile: VideoOutputProfile,
) -> Result<bool, MediaError> {
    let video_path = video_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=codec_name,pix_fmt",
                "-of",
                "default=nokey=0:noprint_wrappers=1",
            ])
            .arg(&video_path)
            .output()
            .map_err(MediaError::Io)?;

        if !output.status.success() {
            eprintln!(
                "[media-verify] ffprobe failed for '{}': {}",
                video_path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim();

        if line.is_empty() {
            eprintln!(
                "[media-verify] no video stream found in '{}'",
                video_path.display()
            );
            return Ok(false);
        }

        let mut codec = None;
        let mut pix_fmt = None;

        for probe_line in line.lines() {
            if let Some(value) = probe_line.strip_prefix("codec_name=") {
                codec = Some(value.trim().to_string());
            }
            if let Some(value) = probe_line.strip_prefix("pix_fmt=") {
                pix_fmt = Some(value.trim().to_string());
            }
        }

        let Some(codec) = codec else {
            eprintln!(
                "[media-verify] missing codec_name in ffprobe output for '{}': {}",
                video_path.display(),
                line
            );
            return Ok(false);
        };
        let Some(pix_fmt) = pix_fmt else {
            eprintln!(
                "[media-verify] missing pix_fmt in ffprobe output for '{}': {}",
                video_path.display(),
                line
            );
            return Ok(false);
        };

        let expected_codec = match expected_profile {
            VideoOutputProfile::Mp4Compatible
            | VideoOutputProfile::MovFast
            | VideoOutputProfile::MovHighQuality => "h264",
            VideoOutputProfile::LinuxWebm => "vp9",
        };

        if codec != expected_codec {
            eprintln!(
                "[media-verify] codec mismatch for '{}': expected={expected_codec} actual={codec}",
                video_path.display()
            );
            return Ok(false);
        }

        if pix_fmt != "yuv420p" {
            eprintln!(
                "[media-verify] pix_fmt mismatch for '{}': expected=yuv420p actual={pix_fmt}",
                video_path.display()
            );
            return Ok(false);
        }

        Ok(true)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use tempfile::tempdir;

    use super::{
        build_ffmpeg_metadata_args, build_ffmpeg_overlay_args, build_image_overlay_filter,
        build_video_overlay_filter, cleanup_intermediate_files, media_kind_from_path,
        normalize_datetime_for_ffmpeg, parse_coordinates, transpose_value, HwAccelEncoder,
        HwAccelPreference, ImageOutputFormat, ImageProbe, ImageQuality, MediaEncodingOptions,
        MediaKind, MediaProbe, OverlayStrategy, VideoOutputProfile,
    };

    #[test]
    fn detects_supported_media_kinds() {
        assert_eq!(
            media_kind_from_path(Path::new("memory.jpg")),
            Some(MediaKind::Image)
        );
        assert_eq!(
            media_kind_from_path(Path::new("memory.JPEG")),
            Some(MediaKind::Image)
        );
        assert_eq!(
            media_kind_from_path(Path::new("memory.mp4")),
            Some(MediaKind::Video)
        );
        assert_eq!(
            media_kind_from_path(Path::new("memory.mov")),
            Some(MediaKind::Video)
        );
        assert_eq!(
            media_kind_from_path(Path::new("memory.webm")),
            Some(MediaKind::Video)
        );
    }

    #[test]
    fn builds_overlay_arguments_for_images() {
        let args = build_ffmpeg_overlay_args(
            Path::new("base.jpg"),
            Path::new("overlay.png"),
            Path::new("output.jpg"),
            MediaKind::Image,
            MediaEncodingOptions {
                video_profile: VideoOutputProfile::Mp4Compatible,
                image_format: ImageOutputFormat::Jpg,
                image_quality: ImageQuality::Full,
                hw_accel: HwAccelPreference::Disabled,
                overlay_strategy: OverlayStrategy::Upscale,
            },
            None,
            None,
        );

        assert!(args.contains(&"-frames:v".to_string()));
        assert!(!args.contains(&"libx264".to_string()));
    }

    #[test]
    fn builds_overlay_arguments_for_videos() {
        let args = build_ffmpeg_overlay_args(
            Path::new("base.mp4"),
            Path::new("overlay.png"),
            Path::new("output.mp4"),
            MediaKind::Video,
            MediaEncodingOptions {
                video_profile: VideoOutputProfile::Mp4Compatible,
                image_format: ImageOutputFormat::Jpg,
                image_quality: ImageQuality::Full,
                hw_accel: HwAccelPreference::Disabled,
                overlay_strategy: OverlayStrategy::Upscale,
            },
            None,
            None,
        );

        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"0:a?".to_string()));
        assert!(!args.contains(&"-frames:v".to_string()));
    }

    #[test]
    fn normalizes_rfc3339_datetime_for_ffmpeg() {
        let formatted = normalize_datetime_for_ffmpeg("2024-03-01T12:13:14Z")
            .expect("datetime should be normalized");
        assert_eq!(formatted, "2024:03:01 12:13:14");
    }

    #[test]
    fn parses_coordinates_from_location_string() {
        let coordinates = parse_coordinates("48.8566,2.3522").expect("coordinates should parse");
        assert_eq!(coordinates.0, 48.8566);
        assert_eq!(coordinates.1, 2.3522);
    }

    #[test]
    fn builds_metadata_arguments_with_gps_tags() {
        let args = build_ffmpeg_metadata_args(
            Path::new("output.jpg"),
            Path::new("output.jpg.metadata.tmp"),
            "2024:03:01 12:13:14",
            Some((12.34, -56.78)),
            MediaKind::Image,
        );

        assert!(args
            .iter()
            .any(|arg| arg.contains("DateTimeOriginal=2024:03:01 12:13:14")));
        assert!(args.iter().any(|arg| arg.contains("GPSLatitude=12.34")));
        assert!(args.iter().any(|arg| arg.contains("GPSLongitude=-56.78")));
    }

    #[tokio::test]
    async fn cleanup_deletes_raw_and_overlay_keeps_final() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let raw_path = temp_dir.path().join("raw.jpg");
        let overlay_path = temp_dir.path().join("overlay.png");
        let final_path = temp_dir.path().join("final.jpg");

        std::fs::write(&raw_path, b"raw").expect("raw file should be created");
        std::fs::write(&overlay_path, b"overlay").expect("overlay file should be created");
        std::fs::write(&final_path, b"final").expect("final file should be created");

        cleanup_intermediate_files(&raw_path, Some(&overlay_path), &final_path)
            .await
            .expect("cleanup should succeed");

        assert!(!raw_path.exists());
        assert!(!overlay_path.exists());
        assert!(final_path.exists());
    }

    #[tokio::test]
    async fn cleanup_does_not_delete_final_when_paths_match() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let final_path = temp_dir.path().join("final.jpg");

        std::fs::write(&final_path, b"final").expect("final file should be created");

        cleanup_intermediate_files(&final_path, Some(&final_path), &final_path)
            .await
            .expect("cleanup should succeed when paths match");

        assert!(final_path.exists());
    }

    #[test]
    fn video_overlay_filter_no_rotation_same_resolution() {
        let video = MediaProbe {
            width: 1080,
            height: 1920,
            rotation: 0,
            display_width: 1080,
            display_height: 1920,
            duration_secs: 6.0,
            has_audio: true,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        let filter = build_video_overlay_filter(
            Some(&video),
            Some(&overlay),
            OverlayStrategy::Upscale,
            None,
        );
        assert!(filter.contains("scale=1080:1920"));
        assert!(!filter.contains("transpose"));
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn video_overlay_filter_rotation_90() {
        let video = MediaProbe {
            width: 960,
            height: 540,
            rotation: 90,
            display_width: 540,
            display_height: 960,
            duration_secs: 6.0,
            has_audio: true,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        let filter = build_video_overlay_filter(
            Some(&video),
            Some(&overlay),
            OverlayStrategy::Upscale,
            None,
        );
        // Rotation is handled by ffmpeg autorotate, not by transpose in the filter.
        assert!(!filter.contains("transpose"));
        assert!(filter.contains("scale=1080:1920"));
    }

    #[test]
    fn video_overlay_filter_rotation_neg90() {
        let video = MediaProbe {
            width: 960,
            height: 540,
            rotation: -90,
            display_width: 540,
            display_height: 960,
            duration_secs: 6.0,
            has_audio: true,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        let filter = build_video_overlay_filter(
            Some(&video),
            Some(&overlay),
            OverlayStrategy::Upscale,
            None,
        );
        // Rotation is handled by ffmpeg autorotate, not by transpose in the filter.
        assert!(!filter.contains("transpose"));
        assert!(filter.contains("scale=1080:1920"));
    }

    #[test]
    fn video_overlay_filter_upscales_video_to_overlay() {
        let video = MediaProbe {
            width: 540,
            height: 960,
            rotation: 0,
            display_width: 540,
            display_height: 960,
            duration_secs: 4.0,
            has_audio: false,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        let filter = build_video_overlay_filter(
            Some(&video),
            Some(&overlay),
            OverlayStrategy::Upscale,
            None,
        );
        // Target should be 1080x1920 (the larger)
        assert!(filter.contains("scale=1080:1920"));
    }

    #[test]
    fn video_overlay_filter_uses_even_dimensions() {
        let video = MediaProbe {
            width: 539,
            height: 961,
            rotation: 0,
            display_width: 539,
            display_height: 961,
            duration_secs: 4.0,
            has_audio: false,
        };
        let overlay = ImageProbe {
            width: 1079,
            height: 1919,
        };
        let filter = build_video_overlay_filter(
            Some(&video),
            Some(&overlay),
            OverlayStrategy::Upscale,
            None,
        );
        // Both target dims should be even
        assert!(filter.contains("scale=1080:1920"));
    }

    #[test]
    fn video_overlay_filter_fallback_without_probes() {
        let filter = build_video_overlay_filter(None, None, OverlayStrategy::Upscale, None);
        assert!(filter.contains("scale2ref"));
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn image_overlay_filter_scales_to_larger() {
        let base = MediaProbe {
            width: 540,
            height: 960,
            rotation: 0,
            display_width: 540,
            display_height: 960,
            duration_secs: 0.0,
            has_audio: false,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        let filter =
            build_image_overlay_filter(Some(&base), Some(&overlay), OverlayStrategy::Upscale);
        assert!(filter.contains("scale=1080:1920"));
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn image_overlay_filter_fallback_without_probes() {
        let filter = build_image_overlay_filter(None, None, OverlayStrategy::Upscale);
        assert!(filter.contains("overlay=0:0"));
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn transpose_value_maps_correctly() {
        assert_eq!(transpose_value(0), None);
        assert_eq!(transpose_value(90), Some("1"));
        assert_eq!(transpose_value(-90), Some("2"));
        assert_eq!(transpose_value(270), Some("2"));
        assert_eq!(transpose_value(-270), Some("1"));
        assert!(transpose_value(180).is_some());
    }

    #[test]
    fn video_overlay_args_use_autorotate_and_duration() {
        let video = MediaProbe {
            width: 960,
            height: 540,
            rotation: -90,
            display_width: 540,
            display_height: 960,
            duration_secs: 6.0,
            has_audio: true,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        let args = build_ffmpeg_overlay_args(
            Path::new("base.mp4"),
            Path::new("overlay.png"),
            Path::new("output.mp4"),
            MediaKind::Video,
            MediaEncodingOptions {
                video_profile: VideoOutputProfile::Mp4Compatible,
                image_format: ImageOutputFormat::Jpg,
                image_quality: ImageQuality::Full,
                hw_accel: HwAccelPreference::Disabled,
                overlay_strategy: OverlayStrategy::Upscale,
            },
            Some(&video),
            Some(&overlay),
        );
        // Autorotate should NOT be disabled — let ffmpeg handle rotation.
        assert!(!args.contains(&"-noautorotate".to_string()));
        assert!(args.contains(&"-t".to_string()));
        assert!(!args.contains(&"-shortest".to_string()));
    }

    #[test]
    fn parses_hw_encoders_from_ffmpeg_output() {
        use super::parse_hw_encoders_from_output;

        let output = r#"Encoders:
 V..... libx264              libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 V..... h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)
 V..... h264_qsv             Intel Quick Sync Video H.264 encoder (codec h264)
 V..... h264_vaapi           H.264 (VAAPI) (codec h264)
"#;
        let encoders = parse_hw_encoders_from_output(output);
        assert_eq!(encoders.len(), 3);
        assert_eq!(encoders[0], HwAccelEncoder::Nvenc);
        assert_eq!(encoders[1], HwAccelEncoder::Qsv);
        assert_eq!(encoders[2], HwAccelEncoder::Vaapi);
    }

    #[test]
    fn parses_hw_encoders_empty_when_none_available() {
        use super::parse_hw_encoders_from_output;

        let output = "Encoders:\n V..... libx264 libx264\n";
        let encoders = parse_hw_encoders_from_output(output);
        assert!(encoders.is_empty());
    }

    #[test]
    fn video_overlay_filter_downscale_sharpen() {
        let video = MediaProbe {
            width: 720,
            height: 1280,
            rotation: 0,
            display_width: 720,
            display_height: 1280,
            has_audio: false,
            duration_secs: 0.0,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        // With DownscaleWithSharpen, the overlay (larger) should be scaled down
        let filter = build_video_overlay_filter(
            Some(&video),
            Some(&overlay),
            OverlayStrategy::DownscaleWithSharpen,
            None,
        );
        // Should contain unsharp for sharpening
        assert!(filter.contains("unsharp"));
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn image_overlay_filter_downscale_sharpen() {
        let base = MediaProbe {
            width: 720,
            height: 1280,
            rotation: 0,
            display_width: 720,
            display_height: 1280,
            has_audio: false,
            duration_secs: 0.0,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        let filter = build_image_overlay_filter(
            Some(&base),
            Some(&overlay),
            OverlayStrategy::DownscaleWithSharpen,
        );
        assert!(filter.contains("unsharp"));
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn video_overlay_filter_vaapi_appends_hwupload() {
        let video = MediaProbe {
            width: 1080,
            height: 1920,
            rotation: 0,
            display_width: 1080,
            display_height: 1920,
            has_audio: false,
            duration_secs: 0.0,
        };
        let overlay = ImageProbe {
            width: 1080,
            height: 1920,
        };
        let filter = build_video_overlay_filter(
            Some(&video),
            Some(&overlay),
            OverlayStrategy::Upscale,
            Some(HwAccelEncoder::Vaapi),
        );
        assert!(filter.contains("format=nv12,hwupload"));
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn hw_accel_preference_from_setting() {
        assert_eq!(
            HwAccelPreference::from_setting(Some("auto")),
            HwAccelPreference::Auto,
        );
        assert_eq!(
            HwAccelPreference::from_setting(Some("nvenc")),
            HwAccelPreference::Nvenc,
        );
        assert_eq!(
            HwAccelPreference::from_setting(Some("qsv")),
            HwAccelPreference::Qsv,
        );
        assert_eq!(
            HwAccelPreference::from_setting(Some("vaapi")),
            HwAccelPreference::Vaapi,
        );
        assert_eq!(
            HwAccelPreference::from_setting(Some("disabled")),
            HwAccelPreference::Disabled,
        );
        assert_eq!(
            HwAccelPreference::from_setting(None),
            HwAccelPreference::Auto,
        );
        assert_eq!(
            HwAccelPreference::from_setting(Some("junk")),
            HwAccelPreference::Auto,
        );
    }

    #[test]
    fn overlay_strategy_from_setting() {
        assert_eq!(
            OverlayStrategy::from_setting(Some("upscale")),
            OverlayStrategy::Upscale,
        );
        assert_eq!(
            OverlayStrategy::from_setting(Some("downscale_sharpen")),
            OverlayStrategy::DownscaleWithSharpen,
        );
        assert_eq!(
            OverlayStrategy::from_setting(None),
            OverlayStrategy::Upscale,
        );
        assert_eq!(
            OverlayStrategy::from_setting(Some("invalid")),
            OverlayStrategy::Upscale,
        );
    }
}

use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::media;

#[derive(Debug)]
pub enum ProcessorError {
    Io(std::io::Error),
    Join(tokio::task::JoinError),
    Media(media::MediaError),
    InvalidInput(&'static str),
    FfmpegFailed { status: Option<i32>, stderr: String },
}

impl Display for ProcessorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "processor I/O failed: {error}"),
            Self::Join(error) => write!(f, "processor thread failed: {error}"),
            Self::Media(error) => write!(f, "media operation failed: {error}"),
            Self::InvalidInput(reason) => write!(f, "invalid processor input: {reason}"),
            Self::FfmpegFailed { status, stderr } => {
                write!(f, "ffmpeg exited with status {:?}: {}", status, stderr.trim())
            }
        }
    }
}

impl std::error::Error for ProcessorError {}

impl From<std::io::Error> for ProcessorError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<tokio::task::JoinError> for ProcessorError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Join(value)
    }
}

impl From<media::MediaError> for ProcessorError {
    fn from(value: media::MediaError) -> Self {
        Self::Media(value)
    }
}

#[derive(Debug, Clone)]
pub struct ProcessMediaInput {
    pub memory_item_id: i64,
    pub raw_media_paths: Vec<PathBuf>,
    pub overlay_path: Option<PathBuf>,
    pub date_taken: String,
    pub location: Option<String>,
    pub export_dir: PathBuf,
    pub thumbnail_dir: PathBuf,
    pub keep_originals: bool,
}

#[derive(Debug, Clone)]
pub struct ProcessMediaOutput {
    pub final_media_path: PathBuf,
    pub thumbnail_path: PathBuf,
}

pub async fn process_media(input: ProcessMediaInput) -> Result<ProcessMediaOutput, ProcessorError> {
    if input.raw_media_paths.is_empty() {
        return Err(ProcessorError::InvalidInput("raw_media_paths is empty"));
    }

    tokio::fs::create_dir_all(&input.export_dir).await?;
    tokio::fs::create_dir_all(&input.thumbnail_dir).await?;

    eprintln!(
        "[processor-debug] process_media start memory_item_id={} raw_count={} overlay_present={} export_dir='{}' keep_originals={}",
        input.memory_item_id,
        input.raw_media_paths.len(),
        input.overlay_path.is_some(),
        input.export_dir.display(),
        input.keep_originals
    );

    let extension = media_extension_from_path(&input.raw_media_paths[0]).unwrap_or("bin");
    let final_media_path = input
        .export_dir
        .join(format!("{}.{}", input.memory_item_id, extension));

    let mut temp_concat_path: Option<PathBuf> = None;

    if input.raw_media_paths.len() > 1 && is_video_extension(extension) {
        let concat_output_path = input
            .export_dir
            .join(format!("{}.concat.{}", input.memory_item_id, extension));

        eprintln!(
            "[processor-debug] concatenating parts memory_item_id={} parts={} concat_output='{}'",
            input.memory_item_id,
            input.raw_media_paths.len(),
            concat_output_path.display()
        );

        concat_video_parts(&input.raw_media_paths, &concat_output_path).await?;
        temp_concat_path = Some(concat_output_path);
    }

    let base_media_path = temp_concat_path
        .as_deref()
        .or_else(|| input.raw_media_paths.first().map(PathBuf::as_path))
        .ok_or(ProcessorError::InvalidInput("missing base media path"))?;

    media::merge_media_with_optional_overlay(
        base_media_path,
        input.overlay_path.as_deref(),
        &final_media_path,
    )
    .await?;

    media::write_metadata_with_ffmpeg(
        &final_media_path,
        &input.date_taken,
        input.location.as_deref(),
    )
    .await?;

    let thumbnail_path = input
        .thumbnail_dir
        .join(format!("{}.webp", input.memory_item_id));

    generate_webp_thumbnail(&final_media_path, &thumbnail_path).await?;

    eprintln!(
        "[processor-debug] process_media success memory_item_id={} final_media='{}' thumbnail='{}'",
        input.memory_item_id,
        final_media_path.display(),
        thumbnail_path.display()
    );

    if !input.keep_originals {
        for raw_media_path in &input.raw_media_paths {
            remove_file_if_exists(raw_media_path).await?;
        }

        if let Some(overlay_path) = input.overlay_path.as_deref() {
            remove_file_if_exists(overlay_path).await?;
        }
    }

    if let Some(temp_concat_path) = temp_concat_path.as_deref() {
        remove_file_if_exists(temp_concat_path).await?;
    }

    Ok(ProcessMediaOutput {
        final_media_path,
        thumbnail_path,
    })
}

async fn concat_video_parts(parts: &[PathBuf], output_path: &Path) -> Result<(), ProcessorError> {
    let parts = parts.to_vec();
    let output_path = output_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let list_path = output_path.with_extension("concat.txt");
        let list_path_for_ffmpeg = std::fs::canonicalize(&list_path).unwrap_or_else(|_| list_path.clone());
        let mut list_content = String::new();

        for part in &parts {
            let absolute_part_path = std::fs::canonicalize(part).unwrap_or_else(|_| part.clone());
            let escaped_path = absolute_part_path.to_string_lossy().replace('\'', "'\\''");
            list_content.push_str(&format!("file '{}'\n", escaped_path));
        }

        std::fs::write(&list_path, list_content)?;

        eprintln!(
            "[processor-debug] ffmpeg concat start parts={} output='{}'",
            parts.len(),
            output_path.display()
        );

        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                &list_path_for_ffmpeg.to_string_lossy(),
                "-c",
                "copy",
                &output_path.to_string_lossy(),
            ])
            .output()
            .map_err(ProcessorError::Io)?;

        let _ = std::fs::remove_file(&list_path);

        if output.status.success() {
            return Ok(());
        }

        Err(ProcessorError::FfmpegFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    })
    .await?
}

async fn generate_webp_thumbnail(
    media_path: &Path,
    thumbnail_path: &Path,
) -> Result<(), ProcessorError> {
    let media_path = media_path.to_path_buf();
    let thumbnail_path = thumbnail_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        if let Some(parent) = thumbnail_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        eprintln!(
            "[processor-debug] thumbnail generation start media='{}' thumbnail='{}'",
            media_path.display(),
            thumbnail_path.display()
        );

        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                &media_path.to_string_lossy(),
                "-vf",
                "scale=300:300:force_original_aspect_ratio=decrease,pad=300:300:(ow-iw)/2:(oh-ih)/2",
                "-frames:v",
                "1",
                &thumbnail_path.to_string_lossy(),
            ])
            .output()
            .map_err(ProcessorError::Io)?;

        if output.status.success() {
            return Ok(());
        }

        Err(ProcessorError::FfmpegFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    })
    .await?
}

async fn remove_file_if_exists(path: &Path) -> Result<(), ProcessorError> {
    if tokio::fs::try_exists(path).await? {
        tokio::fs::remove_file(path).await?;
    }

    Ok(())
}

fn media_extension_from_path(path: &Path) -> Option<&str> {
    path.extension().and_then(|value| value.to_str())
}

fn is_video_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "mp4" | "mov" | "m4v" | "webm"
    )
}

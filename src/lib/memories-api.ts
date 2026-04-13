import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type DownloadErrorCode =
  | "EXPIRED_LINK"
  | "HTTP_ERROR"
  | "IO_ERROR"
  | "CONCURRENCY_ERROR"
  | "STOPPED"
  | "INTERNAL_ERROR";

export type ProcessErrorCode =
  | "MISSING_DOWNLOADED_FILE"
  | "PROCESSING_FAILED";

export type DownloadProgressPayload = {
  totalFiles: number;
  completedFiles: number;
  successfulFiles: number;
  failedFiles: number;
  memoryItemId: number | null;
  status: string;
  errorCode: DownloadErrorCode | null;
  errorMessage: string | null;
};

export type ImportMemoriesResult = {
  parsedCount: number;
  importedCount: number;
  skippedDuplicates: number;
};

export type ExportJobState = {
  status: string;
  totalFiles: number;
  downloadedFiles: number;
};

export type ProcessMemoriesResult = {
  processedCount: number;
  failedCount: number;
  missingCount: number;
};

export type MissingFileItem = {
  memoryGroupId: number;
  memoryItemId: number;
  dateTaken: string;
  mid?: string;
  location?: string;
  mediaDownloadUrl: string;
  lastErrorMessage?: string;
};

export type ProcessProgressPayload = {
  totalFiles: number;
  completedFiles: number;
  successfulFiles: number;
  failedFiles: number;
  memoryItemId: number | null;
  status: string;
  errorCode: ProcessErrorCode | null;
  errorMessage: string | null;
  debugStage?: string | null;
  debugMid?: string | null;
  debugDate?: string | null;
  debugZip?: string | null;
  debugDetails?: string | null;
};

export type ThumbnailItem = {
  memoryItemId: number;
  thumbnailPath: string;
};

export type ViewerMediaKind = "image" | "video";

export type ViewerItem = {
  memoryItemId: number;
  dateTaken: string;
  location?: string;  // resolved location name (city, region, country)
  rawLocation?: string;  // raw coordinates (lat,lon) from export
  thumbnailPath: string;
  mediaPath: string;
  mediaKind: ViewerMediaKind;
  mediaFormat?: string;
};

export type DownloadRateLimitSettings = {
  requestsPerMinute: number;
  concurrentDownloads: number;
};

export type ThumbnailQuality = "360p" | "480p" | "720p" | "1080p";
export type VideoProfile = "auto" | "mp4_compatible" | "linux_webm" | "mov_fast" | "mov_high_quality";
export type ImageOutputFormat = "jpg" | "webp" | "png";
export type ImageQuality = "full" | "balanced" | "fast";

export type ZipSessionInitResult = {
  jobId: string;
  activeZip: string | null;
};

export type ProcessingSessionOverview = {
  jobId: string | null;
  exportStatus: string;
  totalFiles: number;
  downloadedFiles: number;
  processedFiles: number;
  missingFiles: number;
  duplicatesSkipped: number;
  isPaused: boolean;
  isStopped: boolean;
  activeZip: string | null;
  finishedZipFiles: string[];
};

export type SessionLogPayload = {
  message: string;
};

export type ArchiveCreationResult = {
  archivePath: string;
  addedFiles: number;
};

export type ViewerArchiveImportResult = {
  importedCount: number;
  skippedCount: number;
};

export async function importMemoriesJson(
  jsonContent: string,
): Promise<ImportMemoriesResult> {
  return invoke<ImportMemoriesResult>("import_memories_json", { jsonContent });
}

export async function importMemoriesFromZip(zipPath: string): Promise<ImportMemoriesResult> {
  return invoke<ImportMemoriesResult>("import_memories_from_zip", { zipPath });
}

export async function getSystemLocale(): Promise<string | null> {
  return invoke<string | null>("get_system_locale");
}

export type SystemCodecInfo = {
  hasH264Decoder: boolean;
  hasVp9Decoder: boolean;
  hasOpusDecoder: boolean;
  hasAacDecoder: boolean;
  recommendedProfile: string;
  availableHwEncoders: string[];
  recommendedHwEncoder: string | null;
};

export async function probeSystemCodecs(): Promise<SystemCodecInfo> {
  return invoke<SystemCodecInfo>("probe_system_codecs");
}

export async function validateMemoryFile(path: string): Promise<boolean> {
  return invoke<boolean>("validate_memory_file", { path });
}

export async function validateMemoryJsonContent(jsonContent: string): Promise<boolean> {
  return invoke<boolean>("validate_memory_json_content", { jsonContent });
}

export async function validateBaseZipArchive(path: string): Promise<boolean> {
  return invoke<boolean>("validate_base_zip_archive", { path });
}

export async function initializeZipSession(zipPaths: string[]): Promise<ZipSessionInitResult> {
  return invoke<ZipSessionInitResult>("initialize_zip_session", { zipPaths });
}

export async function finalizeZipSession(jobId?: string): Promise<void> {
  return invoke<void>("finalize_zip_session", { jobId });
}

export async function setProcessingPaused(paused: boolean): Promise<void> {
  return invoke<void>("set_processing_paused", { paused });
}

export async function stopProcessingSession(): Promise<void> {
  return invoke<void>("stop_processing_session");
}

export async function resumeProcessingSession(): Promise<void> {
  return invoke<void>("resume_processing_session");
}

export async function getProcessingSessionOverview(): Promise<ProcessingSessionOverview> {
  return invoke<ProcessingSessionOverview>("get_processing_session_overview");
}

export async function getMissingFiles(): Promise<MissingFileItem[]> {
  return invoke<MissingFileItem[]>("get_missing_files");
}

export async function getMissingFileByMemoryItemId(
  memoryItemId: number,
): Promise<MissingFileItem | null> {
  return invoke<MissingFileItem | null>("get_missing_file_by_memory_item_id", {
    memoryItemId,
  });
}

export async function downloadQueuedMemories(
  outputDir: string,
  settings?: DownloadRateLimitSettings,
): Promise<number> {
  return invoke<number>("download_queued_memories", {
    outputDir,
    requestsPerMinute: settings?.requestsPerMinute,
    concurrentDownloads: settings?.concurrentDownloads,
  });
}

export async function resumeExportDownloads(
  outputDir: string,
  settings?: DownloadRateLimitSettings,
): Promise<number> {
  return invoke<number>("resume_export_downloads", {
    outputDir,
    requestsPerMinute: settings?.requestsPerMinute,
    concurrentDownloads: settings?.concurrentDownloads,
  });
}

export async function getJobState(): Promise<ExportJobState> {
  return invoke<ExportJobState>("get_job_state");
}

export async function getQueuedCount(): Promise<number> {
  return invoke<number>("get_queued_count");
}

export type EncodingHwAccel = "auto" | "nvenc" | "qsv" | "vaapi" | "disabled";
export type OverlayStrategy = "upscale" | "downscale_sharpen";

export async function processDownloadedMemories(
  outputDir: string,
  keepOriginals: boolean,
  thumbnailQuality?: ThumbnailQuality,
  videoProfile?: VideoProfile,
  imageOutputFormat?: ImageOutputFormat,
  imageQuality?: ImageQuality,
  encodingHwAccel?: EncodingHwAccel,
  overlayStrategy?: OverlayStrategy,
): Promise<ProcessMemoriesResult> {
  return invoke<ProcessMemoriesResult>("process_downloaded_memories", {
    outputDir,
    keepOriginals,
    thumbnailQuality,
    videoProfile,
    imageOutputFormat,
    imageQuality,
    encodingHwAccel,
    overlayStrategy,
  });
}

export async function processMemoriesFromZipArchives(
  zipPaths: string[],
  outputDir: string,
  keepOriginals: boolean,
  thumbnailQuality?: ThumbnailQuality,
  videoProfile?: VideoProfile,
  imageOutputFormat?: ImageOutputFormat,
  imageQuality?: ImageQuality,
  encodingHwAccel?: EncodingHwAccel,
  overlayStrategy?: OverlayStrategy,
): Promise<ProcessMemoriesResult> {
  return invoke<ProcessMemoriesResult>("process_memories_from_zip_archives", {
    zipPaths,
    outputDir,
    keepOriginals,
    thumbnailQuality,
    videoProfile,
    imageOutputFormat,
    imageQuality,
    encodingHwAccel,
    overlayStrategy,
  });
}

export async function getThumbnails(
  offset: number,
  limit: number,
): Promise<ThumbnailItem[]> {
  return invoke<ThumbnailItem[]>("get_thumbnails", { offset, limit });
}

export async function getViewerItems(
  offset: number,
  limit: number,
): Promise<ViewerItem[]> {
  return invoke<ViewerItem[]>("get_viewer_items", { offset, limit });
}

export async function hasViewerItems(): Promise<boolean> {
  return invoke<boolean>("has_viewer_items");
}

export async function resetAllAppData(): Promise<void> {
  return invoke<void>("reset_all_app_data");
}

export async function createSettingsMediaBackupZip(
  archivePath: string,
): Promise<ArchiveCreationResult> {
  return invoke<ArchiveCreationResult>("create_settings_media_backup_zip", { archivePath });
}

export async function createViewerExportZip(
  archivePath: string,
): Promise<ArchiveCreationResult> {
  return invoke<ArchiveCreationResult>("create_viewer_export_zip", { archivePath });
}

export async function importViewerExportZip(
  archivePath: string,
): Promise<ViewerArchiveImportResult> {
  return invoke<ViewerArchiveImportResult>("import_viewer_export_zip", { archivePath });
}

export async function getMediaStoragePath(): Promise<string> {
  return invoke<string>("get_media_storage_path");
}

export async function openMediaFolder(): Promise<void> {
  return invoke<void>("open_media_folder");
}

export type DiskSpaceInfo = {
  totalBytes: number;
  freeBytes: number;
};

export async function getExportPath(): Promise<string> {
  return invoke<string>("get_export_path");
}

export async function getDefaultExportPath(): Promise<string> {
  return invoke<string>("get_default_export_path");
}

export async function setExportPath(path: string | null): Promise<string> {
  return invoke<string>("set_export_path", { path });
}

export async function getDiskSpace(path: string): Promise<DiskSpaceInfo> {
  return invoke<DiskSpaceInfo>("get_disk_space", { path });
}

export async function getFilesTotalSize(paths: string[]): Promise<number> {
  return invoke<number>("get_files_total_size", { paths });
}

export async function onDownloadProgress(
  callback: (payload: DownloadProgressPayload) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgressPayload>("download-progress", (event) => {
    callback(event.payload);
  });
}

export async function onProcessProgress(
  callback: (payload: ProcessProgressPayload) => void,
): Promise<UnlistenFn> {
  return listen<ProcessProgressPayload>("process-progress", (event) => {
    callback(event.payload);
  });
}

export async function onSessionLog(
  callback: (payload: SessionLogPayload) => void,
): Promise<UnlistenFn> {
  return listen<SessionLogPayload>("session-log", (event) => {
    callback(event.payload);
  });
}

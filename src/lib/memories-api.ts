import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type DownloadErrorCode =
  | "EXPIRED_LINK"
  | "HTTP_ERROR"
  | "IO_ERROR"
  | "CONCURRENCY_ERROR"
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
};

export type DownloadRateLimitSettings = {
  requestsPerMinute: number;
  concurrentDownloads: number;
};

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
  duplicatesSkipped: number;
  isPaused: boolean;
  isStopped: boolean;
  activeZip: string | null;
  finishedZipFiles: string[];
};

export type SessionLogPayload = {
  message: string;
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

export async function processDownloadedMemories(
  outputDir: string,
  keepOriginals: boolean,
): Promise<ProcessMemoriesResult> {
  return invoke<ProcessMemoriesResult>("process_downloaded_memories", {
    outputDir,
    keepOriginals,
  });
}

export async function processMemoriesFromZipArchives(
  zipPaths: string[],
  outputDir: string,
  keepOriginals: boolean,
): Promise<ProcessMemoriesResult> {
  return invoke<ProcessMemoriesResult>("process_memories_from_zip_archives", {
    zipPaths,
    outputDir,
    keepOriginals,
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

export async function resetAllAppData(): Promise<void> {
  return invoke<void>("reset_all_app_data");
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

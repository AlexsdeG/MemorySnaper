import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type DownloadErrorCode =
  | "EXPIRED_LINK"
  | "HTTP_ERROR"
  | "IO_ERROR"
  | "CONCURRENCY_ERROR"
  | "INTERNAL_ERROR";

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
  errorMessage: string | null;
};

export type ThumbnailItem = {
  memoryItemId: number;
  thumbnailPath: string;
};

export type DownloadRateLimitSettings = {
  requestsPerMinute: number;
  concurrentDownloads: number;
};

export async function importMemoriesJson(
  jsonContent: string,
): Promise<ImportMemoriesResult> {
  return invoke<ImportMemoriesResult>("import_memories_json", { jsonContent });
}

export async function validateMemoryFile(path: string): Promise<boolean> {
  return invoke<boolean>("validate_memory_file", { path });
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

export async function processDownloadedMemories(
  outputDir: string,
  keepOriginals: boolean,
): Promise<ProcessMemoriesResult> {
  return invoke<ProcessMemoriesResult>("process_downloaded_memories", {
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

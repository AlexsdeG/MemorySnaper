import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Archive, PackageOpen, FolderOpen } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { DOWNLOADER_SESSION_STORAGE_KEY, readAppSettings, writeAppSettings } from "@/lib/app-settings";
import { ActionBar } from "@/features/downloader/components/ActionBar";
import { LiveConsole } from "@/features/downloader/components/LiveConsole";
import { ProgressOverview } from "@/features/downloader/components/ProgressOverview";
import { ZipSelector } from "@/features/downloader/components/ZipSelector";
import { ZipStatus } from "@/features/downloader/components/ZipStatus";
import { useI18n } from "@/lib/i18n";
import {
  finalizeZipSession,
  getExportPath,
  getProcessingSessionOverview,
  importMemoriesFromZip,
  importViewerExportZip,
  initializeZipSession,
  type DownloadErrorCode,
  onDownloadProgress,
  onProcessProgress,
  onSessionLog,
  processMemoriesFromZipArchives,
  resumeProcessingSession,
  setExportPath,
  setProcessingPaused,
  stopProcessingSession,
  validateBaseZipArchive,
  type DownloadRateLimitSettings,
  type DownloadProgressPayload,
  type ImageOutputFormat,
  type ImageQuality,
  type ThumbnailQuality,
  type ProcessErrorCode,
  type ProcessProgressPayload,
  type VideoProfile,
} from "@/lib/memories-api";

type ImportState = "idle" | "validating" | "running";
type RuntimeProgress = {
  totalFiles: number;
  completedFiles: number;
  successfulFiles: number;
  failedFiles: number;
  status: string;
  errorCode: DownloadErrorCode | ProcessErrorCode | null;
};
type ValidationState = "idle" | "validating" | "valid" | "invalid";
type NoticeTone = "neutral" | "success" | "error";

function loadRateLimitSettings(): DownloadRateLimitSettings | undefined {
  const settings = readAppSettings();

  return {
    requestsPerMinute: settings.requestsPerMinute,
    concurrentDownloads: settings.concurrentDownloads,
  };
}

function loadThumbnailQualitySetting(): ThumbnailQuality {
  const settings = readAppSettings();
  return settings.thumbnailQuality;
}

type ProcessingFormatSettings = {
  videoProfile: VideoProfile;
  imageOutputFormat: ImageOutputFormat;
  imageQuality: ImageQuality;
};

function loadProcessingFormatSettings(): ProcessingFormatSettings {
  const settings = readAppSettings();
  return {
    videoProfile: settings.videoProfile,
    imageOutputFormat: settings.imageOutputFormat,
    imageQuality: settings.imageQuality,
  };
}

export function Workflow() {
  const { t } = useI18n();
  const [selectedZipPaths, setSelectedZipPaths] = useState<string[]>([]);

  const [isPaused, setIsPaused] = useState(false);
  const [isStopped, setIsStopped] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [activeZip, setActiveZip] = useState<string | null>(null);
  const [finishedZipFiles, setFinishedZipFiles] = useState<string[]>([]);
  const [duplicatesSkipped, setDuplicatesSkipped] = useState(0);
  const [processedFiles, setProcessedFiles] = useState(0);
  const [logLines, setLogLines] = useState<string[]>([]);

  const [importState, setImportState] = useState<ImportState>("idle");
  const [totalFiles, setTotalFiles] = useState(0);
  const [downloadedFiles, setDownloadedFiles] = useState(0);
  const [statusMessage, setStatusMessage] = useState<string>(() => t("downloader.workflow.status.idle"));
  const [noticeTone, setNoticeTone] = useState<NoticeTone>("neutral");
  const [validationState, setValidationState] = useState<ValidationState>("idle");
  const [validationMessage, setValidationMessage] = useState<string>("");
  const [downloadProgress, setDownloadProgress] = useState<RuntimeProgress | null>(null);
  const [processProgress, setProcessProgress] = useState<RuntimeProgress | null>(null);
  const [storageHydrated, setStorageHydrated] = useState(false);
  const [isImportingViewerArchive, setIsImportingViewerArchive] = useState(false);
  const [currentExportPath, setCurrentExportPath] = useState<string>("");

  useEffect(() => {
    void getExportPath().then(setCurrentExportPath);
  }, []);

  useEffect(() => {
    try {
      const serialized = window.localStorage.getItem(DOWNLOADER_SESSION_STORAGE_KEY);
      if (!serialized) {
        setStorageHydrated(true);
        return;
      }

      const persisted = JSON.parse(serialized) as Partial<{
        jobId: string | null;
        activeZip: string | null;
        finishedZipFiles: string[];
        duplicatesSkipped: number;
        processedFiles: number;
        totalFiles: number;
        downloadedFiles: number;
        statusMessage: string;
        noticeTone: NoticeTone;
        validationState: ValidationState;
        validationMessage: string;
        logLines: string[];
        isPaused: boolean;
        isStopped: boolean;
      }>;

      setJobId(persisted.jobId ?? null);
      setActiveZip(persisted.activeZip ?? null);
      setFinishedZipFiles(persisted.finishedZipFiles ?? []);
      setDuplicatesSkipped(persisted.duplicatesSkipped ?? 0);
      setProcessedFiles(persisted.processedFiles ?? 0);
      setTotalFiles(persisted.totalFiles ?? 0);
      setDownloadedFiles(persisted.downloadedFiles ?? 0);
      setStatusMessage(persisted.statusMessage ?? t("downloader.workflow.status.idle"));
      setNoticeTone(persisted.noticeTone ?? "neutral");
      setValidationState(persisted.validationState ?? "idle");
      setValidationMessage(persisted.validationMessage ?? "");
      setLogLines(persisted.logLines ?? []);
      setIsPaused(persisted.isPaused ?? false);
      setIsStopped(persisted.isStopped ?? false);
    } catch (error) {
      console.error("[downloader] Failed to restore persisted session state", error);
    } finally {
      setStorageHydrated(true);
    }
  }, [t]);

  useEffect(() => {
    if (!storageHydrated) {
      return;
    }

    try {
      window.localStorage.setItem(
        DOWNLOADER_SESSION_STORAGE_KEY,
        JSON.stringify({
          jobId,
          activeZip,
          finishedZipFiles,
          duplicatesSkipped,
          processedFiles,
          totalFiles,
          downloadedFiles,
          statusMessage,
          noticeTone,
          validationState,
          validationMessage,
          logLines,
          isPaused,
          isStopped,
        }),
      );
    } catch (error) {
      console.error("[downloader] Failed to persist session state", error);
    }
  }, [
    activeZip,
    downloadedFiles,
    duplicatesSkipped,
    finishedZipFiles,
    isPaused,
    isStopped,
    jobId,
    logLines,
    noticeTone,
    processedFiles,
    statusMessage,
    totalFiles,
    validationMessage,
    validationState,
    storageHydrated,
  ]);

  const setNotice = (message: string, tone: NoticeTone = "neutral") => {
    setStatusMessage(message);
    setNoticeTone(tone);
  };

  const pushLogLine = (message: string) => {
    setLogLines((previous) => {
      const next = [...previous, message];
      return next.slice(Math.max(0, next.length - 150));
    });
  };

  const refreshSessionOverview = async () => {
    const overview = await getProcessingSessionOverview();
    setJobId(overview.jobId);
    setTotalFiles(overview.totalFiles);
    setDownloadedFiles(overview.downloadedFiles);
    setProcessedFiles(overview.processedFiles);
    setDuplicatesSkipped(overview.duplicatesSkipped);
    setIsPaused(overview.isPaused);
    setIsStopped(overview.isStopped);
    setActiveZip(overview.activeZip);
    setFinishedZipFiles(overview.finishedZipFiles);
  };

  const resolveUploadErrorMessage = (error: unknown): string => {
    if (error instanceof Error) {
      if (error.message === "ZIP_PATH_REQUIRED") {
        return t("downloader.workflow.error.zipPathRequired");
      }
      if (error.message === "INVALID_ZIP") {
        return t("downloader.workflow.error.invalidZip");
      }
      if (error.message === "INVALID_JSON") {
        return t("downloader.workflow.error.invalidJson");
      }
    }

    return t("downloader.workflow.error.generic");
  };

  const translateDownloadErrorCode = (errorCode: DownloadErrorCode | null): string => {
    if (!errorCode) {
      return t("downloader.workflow.error.generic");
    }

    if (errorCode === "EXPIRED_LINK") {
      return t("downloader.workflow.error.download.EXPIRED_LINK");
    }

    if (errorCode === "HTTP_ERROR") {
      return t("downloader.workflow.error.download.HTTP_ERROR");
    }

    if (errorCode === "IO_ERROR") {
      return t("downloader.workflow.error.download.IO_ERROR");
    }

    if (errorCode === "CONCURRENCY_ERROR") {
      return t("downloader.workflow.error.download.CONCURRENCY_ERROR");
    }

    return t("downloader.workflow.error.download.INTERNAL_ERROR");
  };

  const translateProcessErrorCode = (errorCode: ProcessErrorCode | null): string => {
    if (!errorCode) {
      return t("downloader.workflow.error.generic");
    }

    if (errorCode === "MISSING_DOWNLOADED_FILE") {
      return t("downloader.workflow.error.process.MISSING_DOWNLOADED_FILE");
    }

    return t("downloader.workflow.error.process.PROCESSING_FAILED");
  };

  useEffect(() => {
    const loadOverview = async () => {
      try {
        await refreshSessionOverview();
      } catch (error) {
        console.error("[downloader] Failed to load session overview", error);
        setNotice(t("downloader.workflow.status.loadingJobState"), "error");
      }
    };

    void loadOverview();
  }, [t]);

  useEffect(() => {
    let unlistenDownload: (() => void) | null = null;
    let unlistenProcess: (() => void) | null = null;
    let unlistenSessionLog: (() => void) | null = null;
    let isDisposed = false;

    const startListeners = async () => {
      const downloadUnlisten = await onDownloadProgress((payload: DownloadProgressPayload) => {
        if (payload.status === "error") {
          console.error("[downloader] Download progress error event", payload);
        }

        setDownloadProgress({
          totalFiles: payload.totalFiles,
          completedFiles: payload.completedFiles,
          successfulFiles: payload.successfulFiles,
          failedFiles: payload.failedFiles,
          status: payload.status,
          errorCode: payload.errorCode,
        });
        setTotalFiles(payload.totalFiles);
        setDownloadedFiles(payload.successfulFiles);

        if (payload.status === "error") {
          setNotice(translateDownloadErrorCode(payload.errorCode), "error");
        }
      });

      if (isDisposed) {
        downloadUnlisten();
        return;
      }

      unlistenDownload = downloadUnlisten;

      const processUnlisten = await onProcessProgress((payload: ProcessProgressPayload) => {
        if (payload.status === "error") {
          const conciseReason = payload.errorMessage
            ? payload.errorMessage.slice(0, 400)
            : "unknown processing error";

          console.error(
            `[downloader] Process failed for memoryItemId=${payload.memoryItemId ?? "unknown"} ` +
              `errorCode=${payload.errorCode ?? "unknown"} completed=${payload.completedFiles}/${payload.totalFiles} reason=${conciseReason}`,
          );
          console.error("[downloader] Process progress error event", payload);
          console.error("[downloader] Process progress error details", JSON.stringify(payload, null, 2));
        }

        setProcessProgress({
          totalFiles: payload.totalFiles,
          completedFiles: payload.completedFiles,
          successfulFiles: payload.successfulFiles,
          failedFiles: payload.failedFiles,
          status: payload.status,
          errorCode: payload.errorCode,
        });
        setTotalFiles((previous) => Math.max(previous, payload.totalFiles));
        setProcessedFiles(payload.successfulFiles);

        if (payload.status === "duplicate") {
          setDuplicatesSkipped((previous) => previous + 1);
        }

        if (payload.status === "error") {
          setNotice(translateProcessErrorCode(payload.errorCode), "error");
        }
      });

      if (isDisposed) {
        processUnlisten();
        return;
      }

      unlistenProcess = processUnlisten;

      const sessionLogUnlisten = await onSessionLog((payload) => {
        pushLogLine(payload.message);
      });

      if (isDisposed) {
        sessionLogUnlisten();
        return;
      }

      unlistenSessionLog = sessionLogUnlisten;
    };

    void startListeners();

    return () => {
      isDisposed = true;
      if (unlistenDownload) {
        unlistenDownload();
      }
      if (unlistenProcess) {
        unlistenProcess();
      }
      if (unlistenSessionLog) {
        unlistenSessionLog();
      }
    };
  }, [t]);

  const progressValue = useMemo(() => {
    if (totalFiles <= 0) {
      return 0;
    }

    // During live processing: completedFiles counts all items (success + error + duplicate)
    // After processing / on reload: use processedFiles + duplicatesSkipped as completed work
    const completed = processProgress?.completedFiles ?? (processedFiles + duplicatesSkipped);
    return Math.min(100, Math.round((completed / totalFiles) * 100));
  }, [processProgress?.completedFiles, processedFiles, duplicatesSkipped, totalFiles]);

  const isWorking = importState !== "idle";
  const canStart = selectedZipPaths.length > 0 && (!isWorking || isStopped);
  const canPauseOrStop = isWorking && !isStopped;
  const canClear =
    (selectedZipPaths.length > 0 || jobId !== null || finishedZipFiles.length > 0 || logLines.length > 0) &&
    (!isWorking || isStopped);
  const isViewerImportBlockedByZipSelection = selectedZipPaths.length > 0;

  type ZipSelection = {
    uuid: string;
    partNumber: number | null;
    fileName: string;
    path: string;
  };

  const extractFileNameFromPath = (path: string): string => {
    const normalized = path.replace(/\\/g, "/");
    const segments = normalized.split("/");
    return segments[segments.length - 1] ?? path;
  };

  const parseSnapchatZipName = (fileName: string): { uuid: string; partNumber: number | null } | null => {
    const lowered = fileName.toLowerCase();
    const withoutExtension = lowered.endsWith(".zip") ? lowered.slice(0, -4) : lowered;

    if (!withoutExtension.startsWith("mydata~")) {
      return null;
    }

    const rest = withoutExtension.slice("mydata~".length).trim();
    if (rest.length === 0) {
      return null;
    }

    const maybePart = rest.match(/^(?<uuid>[0-9a-f-]+)\s+(?<part>\d+)$/);
    if (maybePart?.groups?.uuid && maybePart.groups.part) {
      return {
        uuid: maybePart.groups.uuid,
        partNumber: Number(maybePart.groups.part),
      };
    }

    if (/^[0-9a-f-]+$/.test(rest)) {
      return { uuid: rest, partNumber: null };
    }

    return null;
  };

  const buildZipSelection = (paths: string[]): ZipSelection[] => {
    const parsed = paths.map((path) => {
      if (!path || path.trim().length === 0) {
        throw new Error("ZIP_PATH_REQUIRED");
      }

      const fileName = extractFileNameFromPath(path);

      const parsedName = parseSnapchatZipName(fileName);
      if (!parsedName) {
        throw new Error("INVALID_ZIP");
      }

      return {
        uuid: parsedName.uuid,
        partNumber: parsedName.partNumber,
        fileName,
        path,
      } satisfies ZipSelection;
    });

    const mainZips = parsed.filter((entry) => entry.partNumber === null);
    if (mainZips.length !== 1) {
      throw new Error("INVALID_ZIP");
    }

    const mainUuid = mainZips[0].uuid;
    const mixedUuid = parsed.some((entry) => entry.uuid !== mainUuid);
    if (mixedUuid) {
      throw new Error("INVALID_ZIP");
    }

    const ordered = [...parsed].sort((left, right) => {
      const leftPart = left.partNumber ?? 0;
      const rightPart = right.partNumber ?? 0;
      return leftPart - rightPart;
    });

    return ordered;
  };

  const onChangeExportPath = async () => {
    try {
      const picked = await open({
        title: t("settings.form.exportPath.dialogTitle"),
        directory: true,
        multiple: false,
      });
      if (!picked || typeof picked !== "string") {
        return;
      }
      const resolved = await setExportPath(picked);
      setCurrentExportPath(resolved);
      const settings = readAppSettings();
      writeAppSettings({ ...settings, exportPath: picked });
    } catch {
      // silently ignore — not critical in extractor context
    }
  };

  const onPickZipFiles = async () => {
    try {
      const picked = await open({
        multiple: true,
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });

      if (!picked) {
        return;
      }

      const selected = Array.isArray(picked) ? picked : [picked];
      const normalized = selected.filter((value): value is string => typeof value === "string");

      if (normalized.length === 0) {
        setNotice("No ZIP paths were returned by the file picker.", "error");
        return;
      }

      setSelectedZipPaths(normalized);
      setValidationState("idle");
      setValidationMessage("");
      setNotice(`${normalized.length} ZIP file(s) selected.`, "success");
    } catch (error) {
      console.error("[downloader] Failed to open ZIP picker", error);
      setNotice("Could not open file picker. Please restart the app and try again.", "error");
    }
  };

  const onImportViewerArchive = async () => {
    if (isImportingViewerArchive || isViewerImportBlockedByZipSelection || isWorking) {
      return;
    }

    try {
      setIsImportingViewerArchive(true);
      const picked = await open({
        multiple: false,
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });

      if (!picked || typeof picked !== "string") {
        return;
      }

      const result = await importViewerExportZip(picked);
      setNotice(
        t("downloader.workflow.viewerImport.success", {
          importedCount: result.importedCount,
          skippedCount: result.skippedCount,
        }),
        "success",
      );
    } catch (error: unknown) {
      const msg = typeof error === "string" ? error : "";
      const isWrongType =
        msg.includes("unsupported archive type") || msg.includes("manifest");
      setNotice(
        isWrongType
          ? t("downloader.workflow.viewerImport.wrongArchiveType")
          : t("downloader.workflow.viewerImport.error"),
        "error",
      );
    } finally {
      setIsImportingViewerArchive(false);
    }
  };

  const onStartSession = async () => {
    if (selectedZipPaths.length === 0) {
      setNotice("Select Snapchat ZIP exports to continue.", "error");
      return;
    }

    try {
      await resumeProcessingSession();
      setLogLines([]);
      setFinishedZipFiles([]);
      setDuplicatesSkipped(0);
      setProcessedFiles(0);
      setDownloadProgress(null);
      setProcessProgress(null);
      setIsPaused(false);
      setIsStopped(false);

      setImportState("running");
      setValidationState("validating");
      setValidationMessage("Validating Snapchat ZIP set...");
      setNotice("Validating session inputs...");

      const orderedZipSelection = buildZipSelection(selectedZipPaths);
      const mainZip = orderedZipSelection.find((zip) => zip.partNumber === null);
      if (!mainZip) {
        throw new Error("INVALID_ZIP");
      }

      const zipPaths = orderedZipSelection.map((zip) => zip.path);

      const firstZipValid = await validateBaseZipArchive(mainZip.path);
      if (!firstZipValid) {
        throw new Error("INVALID_ZIP");
      }

      const zipSession = await initializeZipSession(zipPaths);
      setJobId(zipSession.jobId);
      setActiveZip(zipSession.activeZip);
      pushLogLine(
        `[${new Date().toISOString().slice(0, 10)}] ZIP session ${zipSession.jobId} initialized (main: ${mainZip.fileName}, parts: ${Math.max(0, zipPaths.length - 1)})`,
      );

      setValidationState("valid");
      setValidationMessage("Main ZIP verified with json/memories_history.json and memories/.");
      setNotice("Importing memories history from main ZIP...", "success");

      const summary = await importMemoriesFromZip(mainZip.path);
      setTotalFiles(summary.importedCount);
      setDownloadedFiles(0);
      if (summary.skippedDuplicates > 0) {
        setDuplicatesSkipped((previous) => previous + summary.skippedDuplicates);
      }
      pushLogLine(
        `[${new Date().toISOString().slice(0, 10)}] Loaded ${summary.parsedCount} memories from memories_history.json; imported ${summary.importedCount} records (${summary.skippedDuplicates} duplicates skipped on import)`,
      );

      const rateLimitSettings = loadRateLimitSettings();
      if (rateLimitSettings) {
        pushLogLine(
          `[${new Date().toISOString().slice(0, 10)}] ZIP-first mode active; network download is fallback only`,
        );
      }

      const thumbnailQuality = loadThumbnailQualitySetting();
      const processingFormatSettings = loadProcessingFormatSettings();
      pushLogLine(
        `[${new Date().toISOString().slice(0, 10)}] Thumbnail quality set to ${thumbnailQuality}`,
      );
      pushLogLine(
        `[${new Date().toISOString().slice(0, 10)}] Video profile=${processingFormatSettings.videoProfile}, image format=${processingFormatSettings.imageOutputFormat}, image quality=${processingFormatSettings.imageQuality}`,
      );

      setNotice(t("downloader.workflow.status.processing"));
      const processResult = await processMemoriesFromZipArchives(
        zipPaths,
        ".raw_cache",
        false,
        thumbnailQuality,
        processingFormatSettings.videoProfile,
        processingFormatSettings.imageOutputFormat,
        processingFormatSettings.imageQuality,
      );
      setNotice(
        t("downloader.workflow.status.processed", {
          processedCount: processResult.processedCount,
          failedCount: processResult.failedCount,
        }),
        processResult.failedCount > 0 ? "error" : "success",
      );

      await finalizeZipSession(zipSession.jobId);
      setFinishedZipFiles(orderedZipSelection.map((zip) => zip.fileName));

      await refreshSessionOverview();
    } catch (error) {
      console.error("Upload failed:", error);
      const message = resolveUploadErrorMessage(error);
      setValidationState("invalid");
      setValidationMessage(message);
      setNotice(message, "error");
    } finally {
      setImportState("idle");
    }
  };

  const onRemoveSelection = () => {
    resumeProcessingSession().catch((error) => {
      console.error("[downloader] Failed to reset backend processing state on clear", error);
    });
    setSelectedZipPaths([]);
    setJobId(null);
    setActiveZip(null);
    setFinishedZipFiles([]);
    setDuplicatesSkipped(0);
    setProcessedFiles(0);
    setTotalFiles(0);
    setDownloadedFiles(0);
    setDownloadProgress(null);
    setProcessProgress(null);
    setIsPaused(false);
    setIsStopped(false);
    setLogLines([]);
    setValidationState("idle");
    setValidationMessage("");
    setNotice(t("downloader.workflow.status.idle"));

    try {
      window.localStorage.removeItem(DOWNLOADER_SESSION_STORAGE_KEY);
    } catch (error) {
      console.error("[downloader] Failed to clear persisted session state", error);
    }
  };

  const onPauseOrResume = async () => {
    try {
      if (isPaused) {
        await resumeProcessingSession();
        setIsPaused(false);
        pushLogLine(`[${new Date().toISOString().slice(0, 10)}] Resume requested`);
      } else {
        await setProcessingPaused(true);
        setIsPaused(true);
        pushLogLine(`[${new Date().toISOString().slice(0, 10)}] Pause requested`);
      }
    } catch (error) {
      console.error("[downloader] pause/resume failed", error);
      setNotice(t("downloader.workflow.error.generic"), "error");
    }
  };

  const onStopSession = async () => {
    try {
      await stopProcessingSession();
      setIsStopped(true);
      setIsPaused(false);
      pushLogLine(`[${new Date().toISOString().slice(0, 10)}] Stop requested`);
      setNotice("Stop requested. Remaining files will stay pending.", "error");
      await refreshSessionOverview();
    } catch (error) {
      console.error("[downloader] stop failed", error);
      setNotice(t("downloader.workflow.error.generic"), "error");
    }
  };

  return (
    <div className="space-y-5">
      {/* ── Path A: Returning user ── */}
      <div className="rounded-xl border bg-card overflow-hidden">
        <div className="flex items-start gap-4 p-5">
          <div className="mt-0.5 shrink-0 rounded-lg bg-emerald-500/10 p-2.5">
            <Archive className="h-5 w-5 text-emerald-600 dark:text-emerald-400" />
          </div>
          <div className="min-w-0 flex-1 space-y-1">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-sm font-semibold">{t("downloader.workflow.path.returningTitle")}</span>
              <Badge variant="secondary" className="px-1.5 py-0 text-[10px] leading-tight">
                {t("downloader.workflow.path.returningBadge")}
              </Badge>
            </div>
            <p className="text-xs leading-relaxed text-muted-foreground">
              {t("downloader.workflow.path.returningHint")}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3 border-t bg-muted/30 px-5 py-3">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => { void onImportViewerArchive(); }}
            disabled={
              isImportingViewerArchive ||
              isViewerImportBlockedByZipSelection ||
              isWorking
            }
            className="gap-1.5"
          >
            <Archive className="h-3.5 w-3.5" />
            {isImportingViewerArchive
              ? t("downloader.workflow.viewerImport.inProgress")
              : t("downloader.workflow.viewerImport.button")}
          </Button>
          {isViewerImportBlockedByZipSelection && (
            <p className="text-[11px] text-muted-foreground">
              {t("downloader.workflow.viewerImport.disabledReason")}
            </p>
          )}
        </div>
      </div>

      {/* ── OR divider ── */}
      <div className="relative flex items-center gap-3 py-1">
        <div className="h-px flex-1 bg-border" />
        <span className="shrink-0 select-none text-xs font-bold uppercase tracking-[0.25em] text-muted-foreground/70">
          {t("downloader.workflow.choice.or")}
        </span>
        <div className="h-px flex-1 bg-border" />
      </div>

      {/* ── Path B: New user ── */}
      <div className="rounded-xl border bg-card overflow-hidden">
        <div className="flex items-start gap-4 p-5">
          <div className="mt-0.5 shrink-0 rounded-lg bg-primary/10 p-2.5">
            <PackageOpen className="h-5 w-5 text-primary" />
          </div>
          <div className="min-w-0 flex-1 space-y-1">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-sm font-semibold">{t("downloader.workflow.path.newTitle")}</span>
              <Badge variant="secondary" className="px-1.5 py-0 text-[10px] leading-tight">
                {t("downloader.workflow.path.newBadge")}
              </Badge>
            </div>
            <p className="text-xs leading-relaxed text-muted-foreground">
              {t("downloader.workflow.path.newHint")}
            </p>
          </div>
        </div>
        <div className="border-t px-5 py-4">
          <ZipSelector
            selectedZipPaths={selectedZipPaths}
            validationState={validationState}
            validationMessage={validationMessage}
            isWorking={isWorking}
            onPickZipFiles={() => { void onPickZipFiles(); }}
            onRemoveSelection={onRemoveSelection}
            extractFileNameFromPath={extractFileNameFromPath}
            noCard
          />
        </div>
      </div>

      {/* Export path display */}
      {currentExportPath && (
        <div className="flex items-center gap-2 rounded-lg border bg-card px-3 py-2">
          <FolderOpen className="h-4 w-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={currentExportPath}>
            <span className="font-medium text-foreground">{t("downloader.exportPath.label")}:</span>{" "}
            {currentExportPath}
          </span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => { void onChangeExportPath(); }}
            disabled={isWorking}
            className="h-6 shrink-0 px-2 text-[11px] text-muted-foreground hover:text-primary"
          >
            {t("downloader.exportPath.change")}
          </Button>
        </div>
      )}

      {/* Action Bar */}
      <ActionBar
        canStart={canStart}
        canPauseOrStop={canPauseOrStop}
        isPaused={isPaused}
        isStopped={isStopped}
        isWorking={isWorking}
        canClear={canClear}
        onStart={() => { void onStartSession(); }}
        onPauseOrResume={() => { void onPauseOrResume(); }}
        onStop={() => { void onStopSession(); }}
        onClearSession={onRemoveSelection}
      />

      <Separator />

      {/* Progress Overview */}
      <ProgressOverview
        progressValue={progressValue}
        totalFiles={totalFiles}
        processedFiles={processedFiles}
        downloadedFiles={downloadedFiles}
        duplicatesSkipped={duplicatesSkipped}
        downloadProgress={downloadProgress}
        processProgress={processProgress}
        isPaused={isPaused}
        isStopped={isStopped}
        importState={importState}
      />

      {/* Status Notice */}
      {statusMessage && (
        <div className="flex items-center gap-2">
          <Badge
            variant={
              noticeTone === "success"
                ? "default"
                : noticeTone === "error"
                  ? "destructive"
                  : "secondary"
            }
            className="text-xs"
          >
            {noticeTone === "success"
              ? t("downloader.notice.success")
              : noticeTone === "error"
                ? t("downloader.notice.error")
                : t("downloader.notice.info")}
          </Badge>
          <p className="text-xs text-muted-foreground truncate">{statusMessage}</p>
        </div>
      )}

      {/* ZIP Status */}
      <ZipStatus
        finishedZipFiles={finishedZipFiles}
        activeZip={activeZip}
      />

      {/* Live Console */}
      <LiveConsole logLines={logLines} />
    </div>
  );
}


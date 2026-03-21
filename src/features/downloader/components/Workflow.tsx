import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { DOWNLOADER_SESSION_STORAGE_KEY, readAppSettings } from "@/lib/app-settings";
import { useI18n } from "@/lib/i18n";
import {
  finalizeZipSession,
  getProcessingSessionOverview,
  importMemoriesFromZip,
  initializeZipSession,
  type DownloadErrorCode,
  onDownloadProgress,
  onProcessProgress,
  onSessionLog,
  processMemoriesFromZipArchives,
  resumeProcessingSession,
  setProcessingPaused,
  stopProcessingSession,
  validateBaseZipArchive,
  type DownloadRateLimitSettings,
  type DownloadProgressPayload,
  type ProcessErrorCode,
  type ProcessProgressPayload,
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

  const translateDownloadStatus = (status: string): string => {
    if (status === "idle") {
      return t("downloader.workflow.status.downloadStatus.idle");
    }

    if (status === "running") {
      return t("downloader.workflow.status.downloadStatus.running");
    }

    if (status === "success") {
      return t("downloader.workflow.status.downloadStatus.success");
    }

    if (status === "error") {
      return t("downloader.workflow.status.downloadStatus.error");
    }

    return status;
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

    const completed = processProgress?.completedFiles ?? processedFiles;
    return Math.min(100, Math.round((completed / totalFiles) * 100));
  }, [processProgress?.completedFiles, processedFiles, totalFiles]);

  const isWorking = importState !== "idle";
  const canStart = selectedZipPaths.length > 0 && !isWorking;

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

  const onStartSession = async () => {
    if (selectedZipPaths.length === 0) {
      setNotice("Select Snapchat ZIP exports to continue.", "error");
      return;
    }

    try {
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
      pushLogLine(
        `[${new Date().toISOString().slice(0, 10)}] Imported ${summary.importedCount} records (${summary.skippedDuplicates} duplicates skipped on import)`,
      );

      const rateLimitSettings = loadRateLimitSettings();
      if (rateLimitSettings) {
        pushLogLine(
          `[${new Date().toISOString().slice(0, 10)}] ZIP-first mode active; network download is fallback only`,
        );
      }

      setNotice(t("downloader.workflow.status.processing"));
      const processResult = await processMemoriesFromZipArchives(zipPaths, ".raw_cache", false);
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
    <div className="space-y-4">
      <div className="space-y-2 rounded-md border border-border p-3">
        <p className="text-sm font-medium">Session Inputs</p>
        <p className="text-xs text-muted-foreground">
          Upload Snapchat ZIP exports named like mydata~&lt;uuid&gt; (main) and optional mydata~&lt;uuid&gt; &lt;num&gt; parts.
          The main ZIP must include json/memories_history.json and all ZIPs must include memories/.
        </p>

        <div className="space-y-2">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
            <span className="min-w-28 text-xs text-muted-foreground">ZIPs</span>
            <span className="flex-1 truncate text-sm">
              {selectedZipPaths.length > 0
                ? `${selectedZipPaths.length} ZIP files selected`
                : "No ZIP files selected"}
            </span>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                void onPickZipFiles();
              }}
              disabled={isWorking}
            >
              Select ZIPs
            </Button>
          </div>

          {selectedZipPaths.length > 0 ? (
            <div className="rounded-md border border-border p-2 text-xs text-muted-foreground">
              {selectedZipPaths.map((path) => (
                <p key={path}>{extractFileNameFromPath(path)}</p>
              ))}
            </div>
          ) : null}

          <div className="flex gap-2">
            <Button type="button" onClick={onStartSession} disabled={!canStart}>
              Start Session
            </Button>
            <Button type="button" variant="outline" onClick={onRemoveSelection} disabled={isWorking}>
              Clear Selection
            </Button>
          </div>
        </div>

        {validationState !== "idle" ? (
          <p
            className={`text-xs ${
              validationState === "valid"
                ? "text-green-600"
                : validationState === "invalid"
                  ? "text-red-600"
                  : "text-muted-foreground"
            }`}
          >
            {validationMessage}
          </p>
        ) : null}
      </div>

      <div className="space-y-2">
        <p className="text-sm text-muted-foreground">Global Progress</p>
        <Progress value={progressValue} className="h-2" />

        <div className="grid gap-1 text-xs text-muted-foreground">
          <p>Session Job: {jobId ?? "n/a"}</p>
          <p>Files Processed: {processProgress?.completedFiles ?? processedFiles} / {totalFiles}</p>
          <p>Downloaded Files: {downloadProgress?.successfulFiles ?? downloadedFiles} / {totalFiles}</p>
          <p>Duplicates Skipped: {duplicatesSkipped}</p>
          <p>Active ZIP: {activeZip ?? "n/a"}</p>
          <p>
            Download Status: {translateDownloadStatus(downloadProgress?.status ?? "idle")} · Process Status: {processProgress?.status ?? "idle"}
          </p>
          <p>Paused: {isPaused ? "Yes" : "No"} · Stopped: {isStopped ? "Yes" : "No"}</p>
        </div>
      </div>

      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="outline" onClick={onPauseOrResume}>
          {isPaused ? "Resume" : "Pause"}
        </Button>
        <Button type="button" variant="destructive" onClick={onStopSession}>
          Stop
        </Button>
        <Button
          type="button"
          variant="outline"
          onClick={() => {
            void refreshSessionOverview();
          }}
        >
          Reload Session State
        </Button>
      </div>

      <div className="space-y-2 rounded-md border border-border p-3">
        <p className="text-sm font-medium">ZIP Status</p>
        {finishedZipFiles.length === 0 ? (
          <p className="text-xs text-muted-foreground">No finished ZIP files yet.</p>
        ) : (
          <div className="grid gap-1 text-xs text-muted-foreground">
            {finishedZipFiles.map((zipFile) => (
              <p key={zipFile}>✔ {zipFile}</p>
            ))}
          </div>
        )}
      </div>

      <div className="space-y-2 rounded-md border border-border p-3">
        <p className="text-sm font-medium">Live Console</p>
        <div className="max-h-40 overflow-auto rounded-sm bg-muted/40 p-2 text-xs text-muted-foreground">
          {logLines.length === 0 ? (
            <p>No logs yet.</p>
          ) : (
            logLines.map((line, index) => <p key={`${index}-${line}`}>{line}</p>)
          )}
        </div>
      </div>

      <p
        className={`text-sm ${
          noticeTone === "success"
            ? "text-green-600"
            : noticeTone === "error"
              ? "text-red-600"
              : "text-muted-foreground"
        }`}
      >
        {statusMessage}
      </p>
    </div>
  );
}

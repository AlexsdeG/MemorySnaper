import { useEffect, useMemo, useRef, useState } from "react";
import { open, message } from "@tauri-apps/plugin-dialog";
import { Archive, PackageOpen, FolderOpen } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { DOWNLOADER_SESSION_STORAGE_KEY, readAppSettings, writeAppSettings } from "@/lib/app-settings";
import { ActionBar } from "@/features/downloader/components/ActionBar";
import { LiveConsole } from "@/features/downloader/components/LiveConsole";
import { MissingFilesCard } from "@/features/downloader/components/MissingFilesCard";
import { ProgressOverview } from "@/features/downloader/components/ProgressOverview";
import { ZipSelector } from "@/features/downloader/components/ZipSelector";
import { ZipStatus } from "@/features/downloader/components/ZipStatus";
import { StorageBar, estimateRequiredBytes, formatBytes } from "@/features/downloader/components/StorageBar";
import { Disclaimers } from "@/features/downloader/components/Disclaimers";
import { useIsMobile } from "@/hooks/use-mobile";
import { useI18n } from "@/lib/i18n";
import {
  finalizeZipSession,
  getDiskSpace,
  getExportPath,
  getFilesTotalSize,
  getMissingFileByMemoryItemId,
  getMissingFiles,
  getProcessingSessionOverview,
  importMemoriesFromZip,
  downloadQueuedMemories,
  importViewerExportZip,
  initializeZipSession,
  type DownloadErrorCode,
  onDownloadProgress,
  onProcessProgress,
  onSessionLog,
  processDownloadedMemories,
  processMemoriesFromZipArchives,
  resumeProcessingSession,
  setExportPath,
  setProcessingPaused,
  stopProcessingSession,
  validateBaseZipArchive,
  type DownloadRateLimitSettings,
  type DownloadProgressPayload,
  type EncodingHwAccel,
  type ImageOutputFormat,
  type ImageQuality,
  type MissingFileItem,
  type OverlayStrategy,
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
  encodingHwAccel: EncodingHwAccel;
  overlayStrategy: OverlayStrategy;
};

function loadProcessingFormatSettings(): ProcessingFormatSettings {
  const settings = readAppSettings();
  return {
    videoProfile: settings.videoProfile,
    imageOutputFormat: settings.imageOutputFormat,
    imageQuality: settings.imageQuality,
    encodingHwAccel: settings.encodingHwAccel,
    overlayStrategy: settings.overlayStrategy,
  };
}

function nowTs(): string {
  const now = new Date();
  return `[${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}:${String(now.getSeconds()).padStart(2, "0")}]`;
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
  const [missingFiles, setMissingFiles] = useState(0);
  const [processedFiles, setProcessedFiles] = useState(0);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [missingList, setMissingList] = useState<MissingFileItem[]>([]);
  const [isLoadingMissingList, setIsLoadingMissingList] = useState(false);
  const [isDownloadingMissing, setIsDownloadingMissing] = useState(false);
  const [missingDownloadTarget, setMissingDownloadTarget] = useState(0);

  const downloadedBaseRef = useRef(0);
  const processedBaseRef = useRef(0);
  const isDownloadingMissingRef = useRef(false);
  const isSessionResettingRef = useRef(false);
  const panelStateEpochRef = useRef(0);

  useEffect(() => {
    isDownloadingMissingRef.current = isDownloadingMissing;
  }, [isDownloadingMissing]);

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
  const [storageAcknowledged, setStorageAcknowledged] = useState(false);
  const [hardwareAcknowledged, setHardwareAcknowledged] = useState(false);
  const [estimatedBytes, setEstimatedBytes] = useState(0);

  const isMobile = useIsMobile();

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
        missingFiles: number;
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
      setMissingFiles(persisted.missingFiles ?? 0);
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
          missingFiles,
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
    missingFiles,
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
    const epoch = panelStateEpochRef.current;
    const overview = await getProcessingSessionOverview();

    if (isSessionResettingRef.current || epoch !== panelStateEpochRef.current) {
      return;
    }

    setJobId(overview.jobId);
    setTotalFiles(overview.totalFiles);
    setDownloadedFiles(overview.downloadedFiles);
    setProcessedFiles(overview.processedFiles);
    setMissingFiles(overview.missingFiles);
    setDuplicatesSkipped(overview.duplicatesSkipped);
    setIsPaused(overview.isPaused);
    setIsStopped(overview.isStopped);
    setActiveZip(overview.activeZip);
    setFinishedZipFiles(overview.finishedZipFiles);

    // Sync import state with backend: if there's an active job, we're running
    if (overview.jobId !== null && !overview.isStopped) {
      setImportState("running");
    } else {
      setImportState("idle");
    }
  };

  const refreshMissingList = async () => {
    const epoch = panelStateEpochRef.current;

    if (missingFiles <= 0) {
      setMissingList([]);
      return;
    }

    setIsLoadingMissingList(true);
    try {
      const items = await getMissingFiles();

      if (isSessionResettingRef.current || epoch !== panelStateEpochRef.current) {
        return;
      }

      setMissingList(items);
    } catch (error) {
      console.error("[downloader] Failed to load missing files list", error);
      setMissingList([]);
    } finally {
      setIsLoadingMissingList(false);
    }
  };

  const appendMissingListItem = async (memoryItemId: number) => {
    const epoch = panelStateEpochRef.current;

    if (!Number.isFinite(memoryItemId) || memoryItemId <= 0) {
      return;
    }

    try {
      const item = await getMissingFileByMemoryItemId(memoryItemId);

      if (isSessionResettingRef.current || epoch !== panelStateEpochRef.current) {
        return;
      }

      if (!item) {
        return;
      }

      setMissingList((previous) => {
        if (previous.some((entry) => entry.memoryItemId === item.memoryItemId)) {
          return previous;
        }

        const next = [...previous, item];
        next.sort((left, right) => left.memoryGroupId - right.memoryGroupId);
        return next;
      });
    } catch (error) {
      console.error("[downloader] Failed to append missing file item", error);
    }
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

    if (errorCode === "STOPPED") {
      return "Download stopped by user.";
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
        const overview = await getProcessingSessionOverview();

        if (isSessionResettingRef.current) {
          return;
        }

        setJobId(overview.jobId);
        setTotalFiles(overview.totalFiles);
        setDownloadedFiles(overview.downloadedFiles);
        setProcessedFiles(overview.processedFiles);
        setMissingFiles(overview.missingFiles);
        setDuplicatesSkipped(overview.duplicatesSkipped);
        setIsPaused(overview.isPaused);
        setIsStopped(overview.isStopped);
        setActiveZip(overview.activeZip);
        setFinishedZipFiles(overview.finishedZipFiles);

        if (overview.jobId !== null && !overview.isStopped) {
          setImportState("running");
        } else {
          setImportState("idle");
          // If the backend marked the previous session as interrupted (stopped
          // because nothing is running after restart), reset any stale status
          // message that was persisted to localStorage.
          if (overview.isStopped && overview.jobId !== null) {
            setNotice(t("downloader.workflow.status.idle"));
          }
        }
      } catch (error) {
        console.error("[downloader] Failed to load session overview", error);
        setNotice(t("downloader.workflow.status.loadingJobState"), "error");
      }
    };

    void loadOverview();
  }, [t]);

  useEffect(() => {
    if (missingFiles <= 0 || isLoadingMissingList || missingList.length > 0) {
      return;
    }

    void refreshMissingList();
  }, [isLoadingMissingList, missingFiles, missingList.length]);

  useEffect(() => {
    let unlistenDownload: (() => void) | null = null;
    let unlistenProcess: (() => void) | null = null;
    let unlistenSessionLog: (() => void) | null = null;
    let isDisposed = false;

    const startListeners = async () => {
      const downloadUnlisten = await onDownloadProgress((payload: DownloadProgressPayload) => {
        if (isSessionResettingRef.current) {
          return;
        }

        const isMissingRun = isDownloadingMissingRef.current;
        const cumulativeDownloaded = isMissingRun
          ? downloadedBaseRef.current + payload.successfulFiles
          : payload.successfulFiles;

        if (payload.status === "success") {
          pushLogLine(
            `${nowTs()} [DOWNLOAD] ${payload.completedFiles}/${payload.totalFiles} · ✓${payload.successfulFiles} ok · ✗${payload.failedFiles} failed`,
          );
        }

        if (payload.status === "stopped") {
          pushLogLine(`${nowTs()} [INFO] Download stopped by user`);
        }

        if (payload.status === "error") {
          console.error("[downloader] Download progress error event", payload);
          pushLogLine(`${nowTs()} [ERROR] Download failed: ${translateDownloadErrorCode(payload.errorCode)}`);
        }

        setDownloadProgress({
          totalFiles: isMissingRun ? missingDownloadTarget : payload.totalFiles,
          completedFiles: payload.completedFiles,
          successfulFiles: isMissingRun ? payload.successfulFiles : cumulativeDownloaded,
          failedFiles: payload.failedFiles,
          status: payload.status,
          errorCode: payload.errorCode,
        });
        setDownloadedFiles(cumulativeDownloaded);

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
        if (isSessionResettingRef.current) {
          return;
        }

        if (payload.status === "error") {
          const debugStage = payload.debugStage ?? "unknown";
          const debugDate = payload.debugDate ?? "unknown";
          const debugMid = payload.debugMid ?? "unknown";
          const debugZip = payload.debugZip ?? "unknown";
          const conciseReason = payload.errorMessage
            ? payload.errorMessage.slice(0, 400)
            : "unknown processing error";

          console.log(
            `[downloader] process-error stage=${debugStage} date=${debugDate} mid=${debugMid} zip=${debugZip} reason=${conciseReason}`,
          );

          console.error(
            `[downloader] Process failed for memoryItemId=${payload.memoryItemId ?? "unknown"} ` +
              `errorCode=${payload.errorCode ?? "unknown"} stage=${debugStage} date=${debugDate} mid=${debugMid} zip=${debugZip} ` +
              `completed=${payload.completedFiles}/${payload.totalFiles} reason=${conciseReason}`,
          );
          console.error("[downloader] Process progress error event", payload);
          console.error("[downloader] Process progress debug context", {
            stage: payload.debugStage ?? null,
            date: payload.debugDate ?? null,
            mid: payload.debugMid ?? null,
            zip: payload.debugZip ?? null,
            details: payload.debugDetails ?? null,
          });
          if (payload.debugDetails) {
            console.error("[downloader] Process progress backend details", payload.debugDetails);
          }
          console.error("[downloader] Process progress error details", JSON.stringify(payload, null, 2));

          pushLogLine(`${nowTs()} [ERROR] Processing failed: ${translateProcessErrorCode(payload.errorCode)} · mid=${debugMid} · date=${debugDate} · stage=${debugStage}`);
        } else if (payload.status === "success" && payload.debugStage === "process.success.overlay_fallback") {
          const debugDate = payload.debugDate ?? "unknown";
          const debugMid = payload.debugMid ?? "unknown";
          const debugZip = payload.debugZip ?? "unknown";
          const fallbackReason = payload.debugDetails
            ? payload.debugDetails.slice(0, 600)
            : "overlay fallback without explicit reason";

          console.log(
            `[downloader] process-overlay-fallback memoryItemId=${payload.memoryItemId ?? "unknown"} date=${debugDate} mid=${debugMid} zip=${debugZip} reason=${fallbackReason}`,
          );
          pushLogLine(
            `${nowTs()} [WARN] Overlay fallback · mid=${debugMid} · date=${debugDate} · zip=${debugZip}`,
          );
        } else if (payload.status === "success") {
          const mid = payload.debugMid;
          const date = payload.debugDate;
          const parts: string[] = [];
          if (mid) parts.push(`mid=${mid}`);
          if (date) parts.push(`date=${date}`);
          const meta = parts.length > 0 ? ` · ${parts.join(" · ")}` : ` · item ${payload.completedFiles}/${payload.totalFiles}`;
          pushLogLine(`${nowTs()} [IMPORT]${meta}`);
        } else if (payload.status === "duplicate") {
          const mid = payload.debugMid;
          const date = payload.debugDate;
          const parts: string[] = [];
          if (mid) parts.push(`mid=${mid}`);
          if (date) parts.push(`date=${date}`);
          const meta = parts.length > 0 ? ` · ${parts.join(" · ")}` : "";
          pushLogLine(`${nowTs()} [SKIP] Duplicate skipped${meta}`);
        } else if (payload.status === "missing") {
          setMissingFiles((previous) => previous + 1);
          if (payload.memoryItemId !== null) {
            void appendMissingListItem(payload.memoryItemId);
          }
          const mid = payload.debugMid;
          const date = payload.debugDate;
          const parts: string[] = [];
          if (mid) parts.push(`mid=${mid}`);
          if (date) parts.push(`date=${date}`);
          const missingMeta = parts.length > 0 ? ` · ${parts.join(" · ")}` : "";
          pushLogLine(`${nowTs()} [MISSING] Not found in ZIPs${missingMeta}`);
        }

        setProcessProgress({
          totalFiles: payload.totalFiles,
          completedFiles: payload.completedFiles,
          successfulFiles: isDownloadingMissingRef.current
            ? processedBaseRef.current + payload.successfulFiles
            : payload.successfulFiles,
          failedFiles: payload.failedFiles,
          status: payload.status,
          errorCode: payload.errorCode,
        });
        setTotalFiles((previous) => Math.max(previous, payload.totalFiles));
        setProcessedFiles((previous) => {
          const next = isDownloadingMissingRef.current
            ? processedBaseRef.current + payload.successfulFiles
            : payload.successfulFiles;
          return Math.max(previous, next);
        });

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
        if (isSessionResettingRef.current) {
          return;
        }

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
  }, [t, missingDownloadTarget]);

  const progressValue = useMemo(() => {
    if (totalFiles <= 0) {
      return 0;
    }

    // During live processing: completedFiles counts all items (success + error + duplicate)
    // After processing / on reload: use processed + duplicate + missing as completed work
    const completed = processProgress?.completedFiles ?? (processedFiles + duplicatesSkipped + missingFiles);
    return Math.min(100, Math.round((completed / totalFiles) * 100));
  }, [processProgress?.completedFiles, processedFiles, duplicatesSkipped, missingFiles, totalFiles]);

  const isWorking = importState !== "idle";
  const showHardwareDisclaimer = isMobile || selectedZipPaths.length > 4;
  const disclaimersAcknowledged =
    storageAcknowledged && (!showHardwareDisclaimer || hardwareAcknowledged);
  const canStart =
    selectedZipPaths.length > 0 && (!isWorking || isStopped) && disclaimersAcknowledged;
  const canPauseOrStop = isWorking && !isStopped;
  const canClear =
    (selectedZipPaths.length > 0 || jobId !== null || finishedZipFiles.length > 0 || logLines.length > 0) &&
    (!isWorking || isStopped);
  const canDownloadAllMissing = !isWorking && (isStopped || finishedZipFiles.length > 0);
  const isViewerImportBlockedByZipSelection = selectedZipPaths.length > 0;
  const areTopWorkflowSectionsDisabled = isWorking || isImportingViewerArchive;

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

  // Snapchat export ids are not always RFC UUIDs; accept alphanumeric ids with optional dashes.
  const SNAPCHAT_EXPORT_ID_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

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

    const maybePart = rest.match(/^(?<uuid>.+)-(?<part>\d+)$/);
    if (
      maybePart?.groups?.uuid
      && maybePart.groups.part
      && SNAPCHAT_EXPORT_ID_PATTERN.test(maybePart.groups.uuid)
    ) {
      return {
        uuid: maybePart.groups.uuid,
        partNumber: Number(maybePart.groups.part),
      };
    }

    if (SNAPCHAT_EXPORT_ID_PATTERN.test(rest)) {
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
      setStorageAcknowledged(false);
      setHardwareAcknowledged(false);
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

    // Pre-start storage check
    if (currentExportPath) {
      try {
        const [space, totalZipSize] = await Promise.all([
          getDiskSpace(currentExportPath),
          getFilesTotalSize(selectedZipPaths),
        ]);
        const estimated = estimateRequiredBytes(totalZipSize);
        if (estimated > space.freeBytes) {
          const result = await message(
            t("downloader.storageBar.insufficientDetail", {
              needed: formatBytes(estimated),
              free: formatBytes(space.freeBytes),
            }),
            {
              title: t("downloader.storageBar.warningTitle"),
              kind: "warning",
              buttons: {
                ok: t("downloader.storageBar.proceedAnyway"),
                cancel: t("downloader.storageBar.cancel"),
              },
            },
          );
          if (result === "Cancel") return;
        }
      } catch {
        // disk space query failed — not a reason to block
      }
    }

    try {
      await resumeProcessingSession();
      setLogLines([]);
      setFinishedZipFiles([]);
      setDuplicatesSkipped(0);
      setMissingFiles(0);
      setMissingList([]);
      setProcessedFiles(0);
      setDownloadProgress(null);
      setProcessProgress(null);
      setIsPaused(false);
      setIsStopped(false);
      downloadedBaseRef.current = 0;
      processedBaseRef.current = 0;

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
        `${nowTs()} [INFO] ZIP session ${zipSession.jobId} initialized · main: ${mainZip.fileName} · parts: ${Math.max(0, zipPaths.length - 1)}`,
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
        `${nowTs()} [INFO] Loaded ${summary.parsedCount} memories · imported ${summary.importedCount} records · ${summary.skippedDuplicates} duplicates skipped`,
      );

      const rateLimitSettings = loadRateLimitSettings();
      if (rateLimitSettings) {
        pushLogLine(
          `${nowTs()} [INFO] ZIP-first mode active; network download is fallback only`,
        );
      }

      const thumbnailQuality = loadThumbnailQualitySetting();
      const processingFormatSettings = loadProcessingFormatSettings();
      pushLogLine(
        `${nowTs()} [INFO] Settings · thumbnail=${thumbnailQuality} · video=${processingFormatSettings.videoProfile} · img-format=${processingFormatSettings.imageOutputFormat} · img-quality=${processingFormatSettings.imageQuality}`,
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
        processingFormatSettings.encodingHwAccel,
        processingFormatSettings.overlayStrategy,
      );
      setNotice(
        t("downloader.workflow.status.processed", {
          processedCount: processResult.processedCount,
          failedCount: processResult.failedCount,
        }),
        processResult.failedCount > 0 ? "error" : "success",
      );

      setMissingFiles(processResult.missingCount);
      setMissingDownloadTarget(processResult.missingCount);

      if (processResult.missingCount > 0) {
        pushLogLine(
          `${nowTs()} [WARN] ${processResult.missingCount} media file(s) not found in provided ZIPs`,
        );
      }

      await finalizeZipSession(zipSession.jobId);
      setFinishedZipFiles(orderedZipSelection.map((zip) => zip.fileName));

      await refreshSessionOverview();
      await refreshMissingList();
      setNotice(
        processResult.missingCount > 0
          ? "ZIP import finished. Review missing files below."
          : "ZIP import finished successfully.",
        "success",
      );
      setIsStopped(true);
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
    isSessionResettingRef.current = true;
    panelStateEpochRef.current += 1;

    resumeProcessingSession().catch((error) => {
      console.error("[downloader] Failed to reset backend processing state on clear", error);
    });

    setSelectedZipPaths([]);
    setImportState("idle");
    setJobId(null);
    setActiveZip(null);
    setFinishedZipFiles([]);
    setDuplicatesSkipped(0);
    setMissingFiles(0);
    setProcessedFiles(0);
    setTotalFiles(0);
    setDownloadedFiles(0);
    setDownloadProgress(null);
    setProcessProgress(null);
    setIsLoadingMissingList(false);
    setIsDownloadingMissing(false);
    setIsPaused(false);
    setIsStopped(false);
    setLogLines([]);
    setValidationState("idle");
    setValidationMessage("");
    setStorageAcknowledged(false);
    setHardwareAcknowledged(false);
    setEstimatedBytes(0);
    setMissingList([]);
    setMissingDownloadTarget(0);
    downloadedBaseRef.current = 0;
    processedBaseRef.current = 0;
    isDownloadingMissingRef.current = false;
    setNotice(t("downloader.workflow.status.idle"));

    try {
      window.localStorage.removeItem(DOWNLOADER_SESSION_STORAGE_KEY);
    } catch (error) {
      console.error("[downloader] Failed to clear persisted session state", error);
    }

    Promise.resolve().then(() => {
      isSessionResettingRef.current = false;
    });
  };

  const onDownloadAllMissing = async () => {
    if (isDownloadingMissing || missingFiles <= 0) {
      return;
    }

    const rateLimitSettings = loadRateLimitSettings();
    const thumbnailQuality = loadThumbnailQualitySetting();
    const processingFormatSettings = loadProcessingFormatSettings();

    try {
      setIsDownloadingMissing(true);
      setImportState("running");
      setIsStopped(false);
      setIsPaused(false);
      downloadedBaseRef.current = downloadedFiles;
      processedBaseRef.current = processedFiles;
      setMissingDownloadTarget(missingFiles);
      await resumeProcessingSession();

      setNotice("Downloading missing files...");
      const downloadedMissingCount = await downloadQueuedMemories(".raw_cache", rateLimitSettings);

      setNotice("Processing downloaded missing files...");
      const missingProcessResult = await processDownloadedMemories(
        ".raw_cache",
        false,
        thumbnailQuality,
        processingFormatSettings.videoProfile,
        processingFormatSettings.imageOutputFormat,
        processingFormatSettings.imageQuality,
        processingFormatSettings.encodingHwAccel,
        processingFormatSettings.overlayStrategy,
      );

      await stopProcessingSession();
      setIsStopped(true);
      await refreshSessionOverview();
      await refreshMissingList();

      setNotice(
        `Downloaded ${downloadedMissingCount} file(s). Processed ${missingProcessResult.processedCount}, failed ${missingProcessResult.failedCount}.`,
        missingProcessResult.failedCount > 0 ? "error" : "success",
      );
      pushLogLine(
        `${nowTs()} [DOWNLOAD] Missing files done · downloaded: ${downloadedMissingCount} · processed: ${missingProcessResult.processedCount} · failed: ${missingProcessResult.failedCount}`,
      );
    } catch (error) {
      console.error("[downloader] failed to download/process missing files", error);
      setNotice(t("downloader.workflow.error.generic"), "error");
    } finally {
      try {
        await stopProcessingSession();
        setIsStopped(true);
      } catch {
        // best-effort stop transition
      }
      setImportState("idle");
      setIsDownloadingMissing(false);
      setMissingDownloadTarget(0);
    }
  };

  const onPauseOrResume = async () => {
    try {
      if (isPaused) {
        await resumeProcessingSession();
        setIsPaused(false);
        pushLogLine(`${nowTs()} [INFO] Resume requested`);
      } else {
        await setProcessingPaused(true);
        setIsPaused(true);
        pushLogLine(`${nowTs()} [INFO] Pause requested`);
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
      pushLogLine(`${nowTs()} [INFO] Stop requested`);
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
      <div
        className={`rounded-xl border bg-card overflow-hidden ${
          areTopWorkflowSectionsDisabled ? "pointer-events-none opacity-60" : ""
        }`}
        aria-disabled={areTopWorkflowSectionsDisabled}
      >
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
      <div
        className={`rounded-xl border bg-card overflow-hidden ${
          areTopWorkflowSectionsDisabled ? "pointer-events-none opacity-60" : ""
        }`}
        aria-disabled={areTopWorkflowSectionsDisabled}
      >
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
            isWorking={areTopWorkflowSectionsDisabled}
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

      {/* Storage bar */}
      {currentExportPath && selectedZipPaths.length > 0 && (
        <StorageBar
          exportPath={currentExportPath}
          zipPaths={selectedZipPaths}
          onEstimatedBytesChange={setEstimatedBytes}
        />
      )}

      {/* Disclaimers */}
      {selectedZipPaths.length > 0 && !isWorking && (
        <Disclaimers
          estimatedBytes={estimatedBytes}
          zipCount={selectedZipPaths.length}
          storageAcknowledged={storageAcknowledged}
          hardwareAcknowledged={hardwareAcknowledged}
          onStorageAcknowledgedChange={setStorageAcknowledged}
          onHardwareAcknowledgedChange={setHardwareAcknowledged}
        />
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
        missingFiles={missingFiles}
        missingDownloadTarget={missingDownloadTarget}
        duplicatesSkipped={duplicatesSkipped}
        downloadProgress={downloadProgress}
        processProgress={processProgress}
        isPaused={isPaused}
        isStopped={isStopped}
        importState={importState}
      />

      {missingFiles > 0 && (
        <MissingFilesCard
          items={missingList}
          isLoading={isLoadingMissingList}
          isDownloading={isDownloadingMissing}
          canDownloadAll={canDownloadAllMissing}
          onDownloadAll={() => { void onDownloadAllMissing(); }}
        />
      )}

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


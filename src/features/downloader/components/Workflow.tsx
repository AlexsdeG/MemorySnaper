import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";

import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { readAppSettings } from "@/lib/app-settings";
import { useI18n } from "@/lib/i18n";
import {
  downloadQueuedMemories,
  type DownloadErrorCode,
  getJobState,
  getQueuedCount,
  importMemoriesJson,
  onDownloadProgress,
  onProcessProgress,
  processDownloadedMemories,
  validateMemoryFile,
  validateMemoryJsonContent,
  type DownloadRateLimitSettings,
  type DownloadProgressPayload,
  type ExportJobState,
  type ProcessErrorCode,
  type ProcessProgressPayload,
} from "@/lib/memories-api";

type ImportState = "idle" | "validating" | "importing";
type WorkflowStage = "download" | "process";
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

type UploadableFile = File & {
  readonly path?: string;
};

function inferWorkflowStage(jobState: ExportJobState): WorkflowStage {
  if (jobState.totalFiles > 0 && jobState.downloadedFiles >= jobState.totalFiles) {
    return "process";
  }

  return "download";
}

function loadRateLimitSettings(): DownloadRateLimitSettings | undefined {
  const settings = readAppSettings();

  return {
    requestsPerMinute: settings.requestsPerMinute,
    concurrentDownloads: settings.concurrentDownloads,
  };
}

export function Workflow() {
  const { t } = useI18n();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [selectedFile, setSelectedFile] = useState<UploadableFile | null>(null);
  const [hasDownloadableData, setHasDownloadableData] = useState(false);
  const [importState, setImportState] = useState<ImportState>("idle");
  const [workflowStage, setWorkflowStage] = useState<WorkflowStage>("download");
  const [jobState, setJobState] = useState<ExportJobState>({
    status: "idle",
    totalFiles: 0,
    downloadedFiles: 0,
  });
  const [statusMessage, setStatusMessage] = useState<string>(() => t("downloader.workflow.status.idle"));
  const [noticeTone, setNoticeTone] = useState<NoticeTone>("neutral");
  const [validationState, setValidationState] = useState<ValidationState>("idle");
  const [validationMessage, setValidationMessage] = useState<string>("");
  const [downloadProgress, setDownloadProgress] = useState<RuntimeProgress | null>(null);
  const [processProgress, setProcessProgress] = useState<RuntimeProgress | null>(null);

  const setNotice = (message: string, tone: NoticeTone = "neutral") => {
    setStatusMessage(message);
    setNoticeTone(tone);
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
    const loadJobState = async () => {
      try {
        const [currentJobState, queuedCount] = await Promise.all([
          getJobState(),
          getQueuedCount(),
        ]);
        setJobState(currentJobState);
        setWorkflowStage(inferWorkflowStage(currentJobState));
        setHasDownloadableData(queuedCount > 0);
      } catch (error) {
        console.error("[downloader] Failed to load initial job state", error);
        setNotice(t("downloader.workflow.status.loadingJobState"), "error");
      }
    };
    void loadJobState();
  }, [t]);

  useEffect(() => {
    let unlistenDownload: (() => void) | null = null;
    let unlistenProcess: (() => void) | null = null;
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

        setJobState((previousState) => ({
          ...previousState,
          totalFiles: payload.totalFiles,
          downloadedFiles: payload.successfulFiles,
          status: payload.status,
        }));

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

        if (payload.status === "error") {
          setNotice(translateProcessErrorCode(payload.errorCode), "error");
        }
      });

      if (isDisposed) {
        processUnlisten();
        return;
      }

      unlistenProcess = processUnlisten;
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
    };
  }, [t]);

  const progressValue = useMemo(() => {
    if (workflowStage === "process") {
      if (!processProgress || processProgress.totalFiles <= 0) {
        return 0;
      }

      return Math.min(
        100,
        Math.round((processProgress.completedFiles / processProgress.totalFiles) * 100),
      );
    }

    if (jobState.totalFiles <= 0) {
      return 0;
    }

    return Math.min(100, Math.round((jobState.downloadedFiles / jobState.totalFiles) * 100));
  }, [jobState.downloadedFiles, jobState.totalFiles, processProgress, workflowStage]);

  const isWorking = importState !== "idle";

  const onUpload = async (fileToUpload: UploadableFile) => {
    console.log(
      `Starting upload for file: ${fileToUpload.name}, size: ${fileToUpload.size} bytes, path: ${fileToUpload.path ?? "N/A"}`,
    );

    const fileName = fileToUpload.name.toLowerCase();
    const isJson = fileName.endsWith(".json");
    const isZip = fileName.endsWith(".zip");

    if (!isJson && !isZip) {
      const message = t("downloader.workflow.status.unsupportedFile");
      setValidationState("invalid");
      setValidationMessage(message);
      setNotice(message, "error");
      setHasDownloadableData(false);
      return;
    }

    try {
      setImportState("validating");
      setValidationState("validating");
      setValidationMessage(t("downloader.workflow.status.validating", { fileName: fileToUpload.name }));
      setNotice(t("downloader.workflow.status.validating", { fileName: fileToUpload.name }));

      let jsonContent: string | null = null;

      if (isZip) {
        if (!fileToUpload.path || fileToUpload.path.trim().length === 0) {
          throw new Error("ZIP_PATH_REQUIRED");
        }

        const isValid = await validateMemoryFile(fileToUpload.path);
        if (!isValid) {
          throw new Error("INVALID_ZIP");
        }
      } else {
        jsonContent = await fileToUpload.text();
        const isValid = await validateMemoryJsonContent(jsonContent);
        if (!isValid) {
          throw new Error("INVALID_JSON");
        }
      }

      setValidationState("valid");
      setValidationMessage(t("downloader.workflow.status.valid", { fileName: fileToUpload.name }));
      setNotice(t("downloader.workflow.status.importing", { fileName: fileToUpload.name }), "success");
      setHasDownloadableData(true);

      setImportState("importing");
      const importedJsonContent = jsonContent ?? (await fileToUpload.text());
      const summary = await importMemoriesJson(importedJsonContent);

      setNotice(
        t("downloader.workflow.status.imported", {
          importedCount: summary.importedCount,
          skippedDuplicates: summary.skippedDuplicates,
        }),
        "success",
      );
      console.log(`Import result: ${summary.importedCount} imported, ${summary.skippedDuplicates} duplicates skipped, ${summary.parsedCount} total parsed.`);

      // Mark import as completed immediately after a successful import call.
      // This guarantees the download button unlocks,
      // even if follow-up state refresh calls fail.
      setHasDownloadableData(true);

      try {
        const [currentJobState, queuedCount] = await Promise.all([
          getJobState(),
          getQueuedCount(),
        ]);
        setJobState(currentJobState);
        setWorkflowStage(inferWorkflowStage(currentJobState));
        setHasDownloadableData(queuedCount > 0);
      } catch (refreshError) {
        console.error("Post-import state refresh failed:", refreshError);
      }
    } catch (error) {
      console.error("Upload failed:", error);
      const message = resolveUploadErrorMessage(error);
      setValidationState("invalid");
      setValidationMessage(message);
      setNotice(message, "error");
      setHasDownloadableData(false);
    } finally {
      setImportState("idle");
    }
  };

  const onFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.item(0) ?? null;
    const uploadableFile = file as UploadableFile | null;
    setSelectedFile(uploadableFile);
    setValidationState("idle");
    setValidationMessage("");

    if (!uploadableFile) {
      setNotice(t("downloader.workflow.status.noFileSelected"));
      return;
    }

    void onUpload(uploadableFile);
  };

  const onRemoveFile = () => {
    setSelectedFile(null);
    setValidationState("idle");
    setValidationMessage("");
    setNotice(t("downloader.workflow.status.idle"));
    if (fileInputRef.current) {
      fileInputRef.current.value = "";
    }
  };

  const onStartDownload = async () => {
    const rateLimitSettings = loadRateLimitSettings();

    console.log("[downloader] Starting queued downloads", {
      outputDir: ".raw_cache",
      settings: rateLimitSettings,
    });

    try {
      setNotice(t("downloader.workflow.status.downloading"));
      setDownloadProgress(null);
      const downloadedCount = await downloadQueuedMemories(".raw_cache", rateLimitSettings);
      const currentJobState = await getJobState();
      setJobState(currentJobState);
      setWorkflowStage(inferWorkflowStage(currentJobState));
      setNotice(t("downloader.workflow.status.downloaded", { count: downloadedCount }), "success");
      console.log("[downloader] Download command completed", {
        downloadedCount,
        jobState: currentJobState,
      });
    } catch (error) {
      console.error("[downloader] downloadQueuedMemories failed", {
        outputDir: ".raw_cache",
        settings: rateLimitSettings,
        error,
      });
      setNotice(t("downloader.workflow.error.generic"), "error");
    }
  };

  const onProcessFiles = async () => {
    try {
      setNotice(t("downloader.workflow.status.processing"));
      setProcessProgress(null);
      const result = await processDownloadedMemories(".raw_cache", true);
      setNotice(
        t("downloader.workflow.status.processed", {
          processedCount: result.processedCount,
          failedCount: result.failedCount,
        }),
        result.failedCount > 0 ? "error" : "success",
      );
    } catch (error) {
      console.error("[downloader] processDownloadedMemories failed", {
        outputDir: ".raw_cache",
        keepOriginals: true,
        error,
      });
      setNotice(t("downloader.workflow.error.generic"), "error");
    }
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2 rounded-md border border-border p-3">
        <p className="text-sm font-medium">{t("downloader.workflow.upload.title")}</p>
        <p className="text-xs text-muted-foreground">
          {t("downloader.workflow.upload.description")}
        </p>
        <input
          ref={fileInputRef}
          type="file"
          accept=".zip,.json,application/json,application/zip"
          onChange={onFileChange}
          className="hidden"
        />
        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          {!selectedFile ? (
            <Button
              type="button"
              variant="outline"
              className="sm:ml-auto"
              onClick={() => fileInputRef.current?.click()}
              disabled={isWorking}
            >
              {t("downloader.workflow.button.upload")}
            </Button>
          ) : (
            <>
              <span className="flex-1 truncate text-sm">{selectedFile.name}</span>
              <Button type="button" variant="outline" onClick={onRemoveFile} disabled={isWorking}>
                {t("downloader.workflow.button.remove")}
              </Button>
            </>
          )}
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
        <p className="text-sm text-muted-foreground">
          {workflowStage === "download"
            ? t("downloader.workflow.progress.download")
            : t("downloader.workflow.progress.processing")}
        </p>
        <Progress value={progressValue} className="h-2" />
        {workflowStage === "download" ? (
          <p className="text-xs text-muted-foreground">
            {t("downloader.workflow.progress.downloadDetails", {
              successful: downloadProgress?.successfulFiles ?? jobState.downloadedFiles,
              total: downloadProgress?.totalFiles ?? jobState.totalFiles,
              status: translateDownloadStatus(downloadProgress?.status ?? jobState.status),
            })}
          </p>
        ) : (
          <p className="text-xs text-muted-foreground">
            {t("downloader.workflow.progress.processDetails", {
              completed: processProgress?.completedFiles ?? 0,
              total: processProgress?.totalFiles ?? 0,
              successful: processProgress?.successfulFiles ?? 0,
              failed: processProgress?.failedFiles ?? 0,
            })}
          </p>
        )}
      </div>

      <div className="flex gap-2">
        {workflowStage === "download" ? (
          <Button
            type="button"
            onClick={onStartDownload}
            disabled={!hasDownloadableData && validationState !== "valid"}
          >
            {t("downloader.workflow.button.startDownload")}
          </Button>
        ) : (
          <Button type="button" variant="outline" onClick={onProcessFiles}>
            {t("downloader.workflow.button.processFiles")}
          </Button>
        )}
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

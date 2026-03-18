import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";

import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  downloadQueuedMemories,
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
};
type ValidationState = "idle" | "validating" | "valid" | "invalid";
type NoticeTone = "neutral" | "success" | "error";

type UploadableFile = File & {
  readonly path?: string;
};

const SETTINGS_STORAGE_KEY = "memorysnaper.rate-limit-settings";

function inferWorkflowStage(jobState: ExportJobState): WorkflowStage {
  if (jobState.totalFiles > 0 && jobState.downloadedFiles >= jobState.totalFiles) {
    return "process";
  }

  return "download";
}

function normalizeError(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return "Operation failed. Please try again.";
}

function loadRateLimitSettings(): DownloadRateLimitSettings | undefined {
  const rawSettings = window.localStorage.getItem(SETTINGS_STORAGE_KEY);
  if (!rawSettings) {
    return undefined;
  }

  try {
    const parsedValue: unknown = JSON.parse(rawSettings);
    if (!parsedValue || typeof parsedValue !== "object") {
      return undefined;
    }

    const requestsPerMinute = Reflect.get(parsedValue, "requestsPerMinute");
    const concurrentDownloads = Reflect.get(parsedValue, "concurrentDownloads");

    if (typeof requestsPerMinute !== "number" || typeof concurrentDownloads !== "number") {
      return undefined;
    }

    return {
      requestsPerMinute,
      concurrentDownloads,
    };
  } catch {
    return undefined;
  }
}

export function Workflow() {
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
  const [statusMessage, setStatusMessage] = useState<string>(
    "Upload a Snapchat export (.zip or .json) to begin.",
  );
  const [noticeTone, setNoticeTone] = useState<NoticeTone>("neutral");
  const [validationState, setValidationState] = useState<ValidationState>("idle");
  const [validationMessage, setValidationMessage] = useState<string>("");
  const [downloadProgress, setDownloadProgress] = useState<RuntimeProgress | null>(null);
  const [processProgress, setProcessProgress] = useState<RuntimeProgress | null>(null);

  const setNotice = (message: string, tone: NoticeTone = "neutral") => {
    setStatusMessage(message);
    setNoticeTone(tone);
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
        setNotice(`Could not load job state: ${normalizeError(error)}`, "error");
      }
    };
    void loadJobState();
  }, []);

  useEffect(() => {
    let unlistenDownload: (() => void) | null = null;
    let unlistenProcess: (() => void) | null = null;

    const startListeners = async () => {
      unlistenDownload = await onDownloadProgress((payload: DownloadProgressPayload) => {
        setDownloadProgress({
          totalFiles: payload.totalFiles,
          completedFiles: payload.completedFiles,
          successfulFiles: payload.successfulFiles,
          failedFiles: payload.failedFiles,
          status: payload.status,
        });

        setJobState((previousState) => ({
          ...previousState,
          totalFiles: payload.totalFiles,
          downloadedFiles: payload.successfulFiles,
          status: payload.status,
        }));
      });

      unlistenProcess = await onProcessProgress((payload: ProcessProgressPayload) => {
        setProcessProgress({
          totalFiles: payload.totalFiles,
          completedFiles: payload.completedFiles,
          successfulFiles: payload.successfulFiles,
          failedFiles: payload.failedFiles,
          status: payload.status,
        });
      });
    };

    void startListeners();

    return () => {
      if (unlistenDownload) {
        unlistenDownload();
      }
      if (unlistenProcess) {
        unlistenProcess();
      }
    };
  }, []);

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
      setValidationState("invalid");
      setValidationMessage("Unsupported file type. Please choose a .zip or .json file.");
      setNotice("Unsupported file type. Please choose a .zip or .json file.", "error");
      setHasDownloadableData(false);
      return;
    }

    try {
      setImportState("validating");
      setValidationState("validating");
      setValidationMessage(`Validating ${fileToUpload.name}...`);
      setNotice(`Validating ${fileToUpload.name}...`);

      let jsonContent: string | null = null;

      if (isZip) {
        if (!fileToUpload.path || fileToUpload.path.trim().length === 0) {
          throw new Error("ZIP validation requires a local file path from the Tauri file picker.");
        }

        const isValid = await validateMemoryFile(fileToUpload.path);
        if (!isValid) {
          throw new Error("ZIP is invalid or does not include memories_history.json.");
        }
      } else {
        jsonContent = await fileToUpload.text();
        const isValid = await validateMemoryJsonContent(jsonContent);
        if (!isValid) {
          throw new Error("JSON is invalid or does not match Snapchat memories schema.");
        }
      }

      setValidationState("valid");
      setValidationMessage(`${fileToUpload.name} is valid.`);
      setNotice(`${fileToUpload.name} is valid. Importing...`, "success");
      setHasDownloadableData(true);

      setImportState("importing");
      const importedJsonContent = jsonContent ?? (await fileToUpload.text());
      const summary = await importMemoriesJson(importedJsonContent);

      setNotice(
        `Imported ${summary.importedCount} items. Skipped ${summary.skippedDuplicates} duplicates.`,
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
      setValidationState("invalid");
      setValidationMessage(normalizeError(error));
      setNotice(normalizeError(error), "error");
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
      setNotice("No file selected.");
      return;
    }

    void onUpload(uploadableFile);
  };

  const onRemoveFile = () => {
    setSelectedFile(null);
    setValidationState("idle");
    setValidationMessage("");
    setNotice("Upload a Snapchat export (.zip or .json) to begin.");
    if (fileInputRef.current) {
      fileInputRef.current.value = "";
    }
  };

  const onStartDownload = async () => {
    try {
      setNotice("Downloading queued media...");
      setDownloadProgress(null);
      const downloadedCount = await downloadQueuedMemories(".raw_cache", loadRateLimitSettings());
      const currentJobState = await getJobState();
      setJobState(currentJobState);
      setWorkflowStage(inferWorkflowStage(currentJobState));
      setNotice(`Downloaded ${downloadedCount} files.`, "success");
    } catch (error) {
      setNotice(normalizeError(error), "error");
    }
  };

  const onProcessFiles = async () => {
    try {
      setNotice("Processing downloaded files...");
      setProcessProgress(null);
      const result = await processDownloadedMemories(".raw_cache", true);
      setNotice(
        `Processed ${result.processedCount} files. Failed ${result.failedCount} files.`,
        result.failedCount > 0 ? "error" : "success",
      );
    } catch (error) {
      setNotice(normalizeError(error), "error");
    }
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2 rounded-md border border-border p-3">
        <p className="text-sm font-medium">Upload Snapchat Export</p>
        <p className="text-xs text-muted-foreground">
          Upload a .zip file (must contain memories_history.json) or a .json file.
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
              Upload
            </Button>
          ) : (
            <>
              <span className="flex-1 truncate text-sm">{selectedFile.name}</span>
              <Button type="button" variant="outline" onClick={onRemoveFile} disabled={isWorking}>
                Remove
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
          {workflowStage === "download" ? "Download progress" : "Processing progress"}
        </p>
        <Progress value={progressValue} className="h-2" />
        {workflowStage === "download" ? (
          <p className="text-xs text-muted-foreground">
            {(downloadProgress?.successfulFiles ?? jobState.downloadedFiles)}/
            {(downloadProgress?.totalFiles ?? jobState.totalFiles)} files downloaded (
            {downloadProgress?.status ?? jobState.status})
          </p>
        ) : (
          <p className="text-xs text-muted-foreground">
            {(processProgress?.completedFiles ?? 0)}/{(processProgress?.totalFiles ?? 0)} files
            processed (ok: {processProgress?.successfulFiles ?? 0}, failed: {processProgress?.failedFiles ?? 0})
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
            Start Download
          </Button>
        ) : (
          <Button type="button" variant="outline" onClick={onProcessFiles}>
            Process Files
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

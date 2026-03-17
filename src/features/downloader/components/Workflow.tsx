import { useEffect, useMemo, useState, type ChangeEvent } from "react";

import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  downloadQueuedMemories,
  getJobState,
  importMemoriesJson,
  onDownloadProgress,
  onProcessProgress,
  processDownloadedMemories,
  validateMemoryFile,
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
  const [selectedFile, setSelectedFile] = useState<UploadableFile | null>(null);
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
  const [downloadProgress, setDownloadProgress] = useState<RuntimeProgress | null>(null);
  const [processProgress, setProcessProgress] = useState<RuntimeProgress | null>(null);

  useEffect(() => {
    const loadJobState = async () => {
      try {
        const currentJobState = await getJobState();
        setJobState(currentJobState);
        setWorkflowStage(inferWorkflowStage(currentJobState));
      } catch (error) {
        setStatusMessage(`Could not load job state: ${normalizeError(error)}`);
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

  const onFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.item(0) ?? null;
    setSelectedFile(file as UploadableFile | null);
    setStatusMessage(file ? `Selected file: ${file.name}` : "No file selected.");
  };

  const onUpload = async () => {
    if (!selectedFile) {
      setStatusMessage("Choose a .zip or .json file first.");
      return;
    }

    const fileName = selectedFile.name.toLowerCase();
    const isJson = fileName.endsWith(".json");
    const isZip = fileName.endsWith(".zip");

    if (!isJson && !isZip) {
      setStatusMessage("Unsupported file type. Please choose a .zip or .json file.");
      return;
    }

    try {
      setImportState("validating");

      if (isZip) {
        if (!selectedFile.path || selectedFile.path.trim().length === 0) {
          throw new Error("ZIP validation requires a local file path from the Tauri file picker.");
        }

        const isValid = await validateMemoryFile(selectedFile.path);
        if (!isValid) {
          throw new Error("ZIP is invalid or does not include memories_history.json.");
        }
      }

      setImportState("importing");
      const jsonContent = await selectedFile.text();
      const summary = await importMemoriesJson(jsonContent);

      setStatusMessage(
        `Imported ${summary.importedCount} items. Skipped ${summary.skippedDuplicates} duplicates.`,
      );

      const currentJobState = await getJobState();
      setJobState(currentJobState);
      setWorkflowStage(inferWorkflowStage(currentJobState));
    } catch (error) {
      setStatusMessage(normalizeError(error));
    } finally {
      setImportState("idle");
    }
  };

  const onStartDownload = async () => {
    try {
      setStatusMessage("Downloading queued media...");
      setDownloadProgress(null);
      const downloadedCount = await downloadQueuedMemories(".raw_cache", loadRateLimitSettings());
      const currentJobState = await getJobState();
      setJobState(currentJobState);
      setWorkflowStage(inferWorkflowStage(currentJobState));
      setStatusMessage(`Downloaded ${downloadedCount} files.`);
    } catch (error) {
      setStatusMessage(normalizeError(error));
    }
  };

  const onProcessFiles = async () => {
    try {
      setStatusMessage("Processing downloaded files...");
      setProcessProgress(null);
      const result = await processDownloadedMemories(".raw_cache", true);
      setStatusMessage(
        `Processed ${result.processedCount} files. Failed ${result.failedCount} files.`,
      );
    } catch (error) {
      setStatusMessage(normalizeError(error));
    }
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2 rounded-md border border-border p-3">
        <p className="text-sm font-medium">Upload Snapchat Export</p>
        <p className="text-xs text-muted-foreground">
          Upload a .zip file (must contain memories_history.json) or a .json file.
        </p>
        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          <input
            type="file"
            accept=".zip,.json,application/json,application/zip"
            onChange={onFileChange}
            className="block w-full text-sm"
          />
          <Button type="button" onClick={onUpload} disabled={!selectedFile || isWorking}>
            {importState === "idle" ? "Upload" : "Uploading..."}
          </Button>
        </div>
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
          <Button type="button" onClick={onStartDownload}>
            Start Download
          </Button>
        ) : (
          <Button type="button" variant="outline" onClick={onProcessFiles}>
            Process Files
          </Button>
        )}
      </div>

      <p className="text-sm text-muted-foreground">{statusMessage}</p>
    </div>
  );
}

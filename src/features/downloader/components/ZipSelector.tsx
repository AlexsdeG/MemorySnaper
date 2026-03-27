import { FolderOpen, X, FileArchive, ShieldCheck, ShieldAlert, Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";

type ValidationState = "idle" | "validating" | "valid" | "invalid";

type ZipSelectorProps = {
  selectedZipPaths: string[];
  validationState: ValidationState;
  validationMessage: string;
  isWorking: boolean;
  onPickZipFiles: () => void;
  onRemoveSelection: () => void;
  extractFileNameFromPath: (path: string) => string;
  noCard?: boolean;
};

export function ZipSelector({
  selectedZipPaths,
  validationState,
  validationMessage,
  isWorking,
  onPickZipFiles,
  onRemoveSelection,
  extractFileNameFromPath,
  noCard = false,
}: ZipSelectorProps) {
  const { t } = useI18n();

  return (
    <div className={noCard ? "space-y-3" : "rounded-lg border bg-card p-4 space-y-3"}>
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-medium">
          <FileArchive className="h-4 w-4 text-muted-foreground" />
          {t("downloader.zipSelector.title")}
        </div>
        {selectedZipPaths.length > 0 && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onRemoveSelection}
            disabled={isWorking}
            className="h-7 text-xs gap-1 text-muted-foreground hover:text-destructive"
          >
            <X className="h-3 w-3" />
            {t("downloader.zipSelector.clear")}
          </Button>
        )}
      </div>

      <p className="text-xs text-muted-foreground leading-relaxed">
        {t("downloader.zipSelector.description")}
      </p>

      {selectedZipPaths.length > 0 ? (
        <div className="space-y-1.5 rounded-md bg-muted/40 p-3">
          {selectedZipPaths.map((path) => (
            <div key={path} className="flex items-center gap-2 text-xs">
              <FileArchive className="h-3 w-3 text-muted-foreground shrink-0" />
              <span className="truncate text-muted-foreground">
                {extractFileNameFromPath(path)}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <button
          type="button"
          onClick={onPickZipFiles}
          disabled={isWorking}
          className="flex w-full items-center justify-center gap-2 rounded-md border-2 border-dashed border-muted-foreground/25 py-6 text-sm text-muted-foreground transition-colors hover:border-primary/50 hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <FolderOpen className="h-5 w-5" />
          {t("downloader.zipSelector.browse")}
        </button>
      )}

      {selectedZipPaths.length > 0 && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onPickZipFiles}
          disabled={isWorking}
          className="gap-1.5"
        >
          <FolderOpen className="h-3.5 w-3.5" />
          {t("downloader.zipSelector.changePick")}
        </Button>
      )}

      {validationState !== "idle" && (
        <div className="flex items-start gap-2">
          {validationState === "validating" && (
            <Loader2 className="h-3.5 w-3.5 mt-0.5 animate-spin text-muted-foreground" />
          )}
          {validationState === "valid" && (
            <ShieldCheck className="h-3.5 w-3.5 mt-0.5 text-emerald-500" />
          )}
          {validationState === "invalid" && (
            <ShieldAlert className="h-3.5 w-3.5 mt-0.5 text-destructive" />
          )}
          <p
            className={`text-xs ${
              validationState === "valid"
                ? "text-emerald-600 dark:text-emerald-400"
                : validationState === "invalid"
                  ? "text-destructive"
                  : "text-muted-foreground"
            }`}
          >
            {validationMessage}
          </p>
        </div>
      )}
    </div>
  );
}

import { useEffect, useRef, useState } from "react";
import { HardDrive, TriangleAlert } from "lucide-react";

import { getDiskSpace, getFilesTotalSize, type DiskSpaceInfo } from "@/lib/memories-api";
import { useI18n } from "@/lib/i18n";

/**
 * Worst-case peak disk space estimate based on actual zip sizes.
 * Extraction ≈ 1× zip size, processed output ≈ 1× zip size, plus a flat
 * 512 MB overhead buffer, all with a 10 % safety margin.
 */
function estimateRequiredBytes(totalZipBytes: number): number {
  const overhead = 512 * 1024 ** 2; // 512 MB flat overhead
  return Math.ceil((totalZipBytes * 2 + overhead) * 1.1);
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) {
    return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  }
  if (bytes >= 1024 ** 2) {
    return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
  }
  return `${(bytes / 1024).toFixed(0)} KB`;
}

type StorageBarProps = {
  exportPath: string;
  zipPaths: string[];
  onEstimatedBytesChange?: (bytes: number) => void;
};

export function StorageBar({ exportPath, zipPaths, onEstimatedBytesChange }: StorageBarProps) {
  const { t } = useI18n();
  const [diskSpace, setDiskSpace] = useState<DiskSpaceInfo | null>(null);
  const [totalZipBytes, setTotalZipBytes] = useState<number>(0);
  const prevEstimated = useRef(0);

  // Serialize zipPaths into a stable key so the effect re-runs on selection changes.
  const zipKey = zipPaths.join("\n");

  useEffect(() => {
    if (!exportPath || zipPaths.length === 0) {
      setDiskSpace(null);
      setTotalZipBytes(0);
      return;
    }

    let cancelled = false;

    const loadStorageInfo = async () => {
      try {
        const [space, size] = await Promise.all([
          getDiskSpace(exportPath),
          getFilesTotalSize(zipPaths),
        ]);

        if (!cancelled) {
          setDiskSpace(space);
          setTotalZipBytes(size);
        }
      } catch (error) {
        // Keep the UI alive even if platform-specific FS probes fail.
        console.error("[downloader] Failed to load storage estimate", error);
        if (!cancelled) {
          setDiskSpace(null);
          setTotalZipBytes(0);
        }
      }
    };

    void loadStorageInfo();

    return () => {
      cancelled = true;
    };
  }, [exportPath, zipKey]);

  useEffect(() => {
    const estimatedBytes =
      diskSpace && zipPaths.length > 0 && totalZipBytes > 0
        ? estimateRequiredBytes(totalZipBytes)
        : 0;

    if (estimatedBytes !== prevEstimated.current) {
      prevEstimated.current = estimatedBytes;
      onEstimatedBytesChange?.(estimatedBytes);
    }
  }, [diskSpace, totalZipBytes, zipPaths.length, onEstimatedBytesChange]);

  if (!diskSpace || zipPaths.length === 0 || totalZipBytes === 0) {
    return null;
  }

  const { totalBytes, freeBytes } = diskSpace;
  const usedBytes = totalBytes - freeBytes;
  const estimatedBytes = estimateRequiredBytes(totalZipBytes);

  const insufficient = estimatedBytes > freeBytes;

  // Percentages for bar segments (clamped to 100%)
  const usedPct = Math.min((usedBytes / totalBytes) * 100, 100);
  const estimatedPct = Math.min((estimatedBytes / totalBytes) * 100, 100 - usedPct);

  return (
    <div className="flex flex-col gap-1.5 rounded-lg border bg-card px-3 py-2.5">
      {/* Bar */}
      <div className="relative h-2 w-full overflow-hidden rounded-full bg-muted">
        {/* Used space */}
        <div
          className="absolute inset-y-0 left-0 rounded-full bg-muted-foreground/40"
          style={{ width: `${usedPct}%` }}
        />
        {/* Estimated needed — overlaid after used */}
        <div
          className={`absolute inset-y-0 rounded-full ${
            insufficient ? "bg-destructive/70" : "bg-primary/60"
          }`}
          style={{ left: `${usedPct}%`, width: `${estimatedPct}%` }}
        />
      </div>

      {/* Labels */}
      <div className="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
        <span className="flex items-center gap-1">
          <HardDrive className="h-3 w-3" />
          {t("downloader.storageBar.freeOfTotal", {
            free: formatBytes(freeBytes),
            total: formatBytes(totalBytes),
          })}
        </span>
        <span className={insufficient ? "font-medium text-destructive" : ""}>
          {t("downloader.storageBar.estimated", { size: formatBytes(estimatedBytes) })}
        </span>
      </div>

      {/* Warning when insufficient */}
      {insufficient && (
        <div className="flex items-start gap-1.5 text-[11px] text-destructive">
          <TriangleAlert className="mt-0.5 h-3 w-3 shrink-0" />
          <span>{t("downloader.storageBar.insufficient")}</span>
        </div>
      )}
    </div>
  );
}

export { estimateRequiredBytes, formatBytes };

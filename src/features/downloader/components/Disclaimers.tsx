import { HardDrive, Monitor } from "lucide-react";

import { Checkbox } from "@/components/ui/checkbox";
import { useI18n } from "@/lib/i18n";
import { useIsMobile } from "@/hooks/use-mobile";
import { formatBytes } from "@/features/downloader/components/StorageBar";

type DisclaimersProps = {
  estimatedBytes: number;
  zipCount: number;
  storageAcknowledged: boolean;
  hardwareAcknowledged: boolean;
  onStorageAcknowledgedChange: (checked: boolean) => void;
  onHardwareAcknowledgedChange: (checked: boolean) => void;
};

export function Disclaimers({
  estimatedBytes,
  zipCount,
  storageAcknowledged,
  hardwareAcknowledged,
  onStorageAcknowledgedChange,
  onHardwareAcknowledgedChange,
}: DisclaimersProps) {
  const { t } = useI18n();
  const isMobile = useIsMobile();
  const showHardwareDisclaimer = isMobile || zipCount > 4;

  return (
    <div className="flex flex-col gap-2">
      {/* Storage disclaimer */}
      <div className="flex items-start gap-3 rounded-lg border-l-4 border-l-primary bg-card px-3 py-2.5">
        <HardDrive className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <p className="text-xs font-medium leading-tight">
            {t("downloader.disclaimer.storage.title")}
          </p>
          <p className="text-[11px] leading-snug text-muted-foreground">
            {t("downloader.disclaimer.storage.body", {
              size: formatBytes(estimatedBytes),
            })}
          </p>
          <label className="flex items-center gap-2 pt-0.5 cursor-pointer">
            <Checkbox
              checked={storageAcknowledged}
              onCheckedChange={(v) => onStorageAcknowledgedChange(v === true)}
            />
            <span className="text-[11px] select-none">
              {t("downloader.disclaimer.acknowledge")}
            </span>
          </label>
        </div>
      </div>

      {/* Hardware disclaimer — shown on mobile or large exports */}
      {showHardwareDisclaimer && (
        <div className="flex items-start gap-3 rounded-lg border-l-4 border-l-amber-500 bg-card px-3 py-2.5">
          <Monitor className="mt-0.5 h-4 w-4 shrink-0 text-amber-500" />
          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
            <p className="text-xs font-medium leading-tight">
              {t("downloader.disclaimer.hardware.title")}
            </p>
            <p className="text-[11px] leading-snug text-muted-foreground">
              {t("downloader.disclaimer.hardware.body")}
            </p>
            <label className="flex items-center gap-2 pt-0.5 cursor-pointer">
              <Checkbox
                checked={hardwareAcknowledged}
                onCheckedChange={(v) => onHardwareAcknowledgedChange(v === true)}
              />
              <span className="text-[11px] select-none">
                {t("downloader.disclaimer.acknowledge")}
              </span>
            </label>
          </div>
        </div>
      )}
    </div>
  );
}

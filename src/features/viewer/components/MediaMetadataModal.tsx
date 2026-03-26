import { X } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";
import type { ViewerMediaKind } from "@/lib/memories-api";

type MetadataItem = {
  id: string;
  dateTaken: string;
  mediaKind: ViewerMediaKind;
  mediaFormat?: string;
  location?: string;  // resolved location name
  rawLocation?: string;  // raw coordinates
};

type MediaMetadataModalProps = {
  open: boolean;
  onClose: () => void;
  item: MetadataItem;
};

export function MediaMetadataModal({ open, onClose, item }: MediaMetadataModalProps) {
  const { t, resolvedLocale } = useI18n();

  useEffect(() => {
    if (!open) {
      return;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopImmediatePropagation();
        event.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => {
      window.removeEventListener("keydown", onKeyDown, { capture: true });
    };
  }, [open, onClose]);

  if (!open) {
    return null;
  }

  const date = new Date(item.dateTaken);
  const isValidDate = !isNaN(date.getTime());

  const formattedDate = isValidDate
    ? new Intl.DateTimeFormat(resolvedLocale, {
        weekday: "long",
        year: "numeric",
        month: "long",
        day: "numeric",
      }).format(date)
    : item.dateTaken;

  const formattedTime = isValidDate
    ? new Intl.DateTimeFormat(resolvedLocale, {
        hour: "2-digit",
        minute: "2-digit",
      }).format(date)
    : "—";

  const typeLabel =
    item.mediaKind === "video"
      ? t("viewer.metadata.type.video")
      : t("viewer.metadata.type.image");
  const typeWithFormat = item.mediaFormat
    ? `${typeLabel}, ${item.mediaFormat.toUpperCase()}`
    : typeLabel;

  return (
    <div
      className="fixed inset-0 z-110 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal
        aria-label={t("viewer.metadata.title")}
        className="relative mx-4 w-full max-w-sm rounded-xl bg-background p-6 shadow-2xl"
      >
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-base font-semibold">{t("viewer.metadata.title")}</h2>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={onClose}
            aria-label={t("viewer.metadata.close")}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>

        <dl className="space-y-3 text-sm">
          <div className="flex items-baseline justify-between gap-4">
            <dt className="shrink-0 text-muted-foreground">{t("viewer.metadata.id")}</dt>
            <dd className="truncate font-mono text-xs">{item.id}</dd>
          </div>
          <div className="flex items-baseline justify-between gap-4">
            <dt className="shrink-0 text-muted-foreground">{t("viewer.metadata.type")}</dt>
            <dd>{typeWithFormat}</dd>
          </div>
          <div className="flex items-baseline justify-between gap-4">
            <dt className="shrink-0 text-muted-foreground">{t("viewer.metadata.date")}</dt>
            <dd className="text-right">{formattedDate}</dd>
          </div>
          <div className="flex items-baseline justify-between gap-4">
            <dt className="shrink-0 text-muted-foreground">{t("viewer.metadata.time")}</dt>
            <dd>{formattedTime}</dd>
          </div>
          <div className="flex items-baseline justify-between gap-4">
            <dt className="shrink-0 text-muted-foreground">{t("viewer.metadata.location")}</dt>
            <dd className="text-right">{item.location ?? t("viewer.metadata.noLocation")}</dd>
          </div>
          {item.rawLocation ? (
            <div className="flex items-baseline justify-between gap-4">
              <dt className="shrink-0 text-muted-foreground">{t("viewer.metadata.coordinates")}</dt>
              <dd className="truncate font-mono text-xs">{item.rawLocation}</dd>
            </div>
          ) : null}
        </dl>
      </div>
    </div>
  );
}

import { X, ChevronLeft, ChevronRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";
import type { ViewerMediaKind } from "@/lib/memories-api";

type MediaViewerModalItem = {
  id: string;
  mediaSrc: string;
  mediaKind: ViewerMediaKind;
};

type MediaViewerModalProps = {
  open: boolean;
  items: MediaViewerModalItem[];
  currentIndex: number;
  onClose: () => void;
  onPrevious: () => void;
  onNext: () => void;
};

export function MediaViewerModal({
  open,
  items,
  currentIndex,
  onClose,
  onPrevious,
  onNext,
}: MediaViewerModalProps) {
  const { t } = useI18n();

  if (!open || items.length === 0 || currentIndex < 0 || currentIndex >= items.length) {
    return null;
  }

  const item = items[currentIndex];
  const isFirst = currentIndex === 0;
  const isLast = currentIndex === items.length - 1;

  return (
    <div className="fixed inset-0 z-[100] bg-black/85 backdrop-blur-sm" role="dialog" aria-modal="true" aria-label={t("viewer.modal.title")}>
      <div className="flex h-full w-full flex-col">
        <header className="flex items-center justify-between px-4 py-3">
          <p className="text-sm text-white/90">
            {t("viewer.modal.counter", {
              current: currentIndex + 1,
              total: items.length,
            })}
          </p>
          <Button
            type="button"
            variant="outline"
            size="icon"
            className="h-9 w-9 border-white/20 bg-black/30 text-white hover:bg-black/50"
            onClick={onClose}
            aria-label={t("viewer.modal.close")}
          >
            <X className="h-4 w-4" />
          </Button>
        </header>

        <div className="relative flex min-h-0 flex-1 items-center justify-center px-3 pb-4 pt-1 sm:px-8">
          <Button
            type="button"
            variant="outline"
            size="icon"
            className="absolute left-2 z-10 h-10 w-10 border-white/20 bg-black/30 text-white hover:bg-black/50 disabled:opacity-30 sm:left-5"
            onClick={onPrevious}
            disabled={isFirst}
            aria-label={t("viewer.modal.previous")}
          >
            <ChevronLeft className="h-5 w-5" />
          </Button>

          <div className="flex h-full w-full items-center justify-center">
            {item.mediaKind === "video" ? (
              <video
                key={item.mediaSrc}
                src={item.mediaSrc}
                className="max-h-full max-w-full rounded-lg object-contain"
                controls
                autoPlay
                muted
                playsInline
              />
            ) : (
              <img
                src={item.mediaSrc}
                alt={t("viewer.modal.imageAlt", { id: item.id })}
                className="max-h-full max-w-full rounded-lg object-contain"
              />
            )}
          </div>

          <Button
            type="button"
            variant="outline"
            size="icon"
            className="absolute right-2 z-10 h-10 w-10 border-white/20 bg-black/30 text-white hover:bg-black/50 disabled:opacity-30 sm:right-5"
            onClick={onNext}
            disabled={isLast}
            aria-label={t("viewer.modal.next")}
          >
            <ChevronRight className="h-5 w-5" />
          </Button>
        </div>
      </div>
    </div>
  );
}

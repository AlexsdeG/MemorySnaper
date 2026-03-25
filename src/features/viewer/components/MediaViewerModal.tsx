import { X, ChevronLeft, ChevronRight, RotateCcw, RotateCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

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
  const [videoLoadError, setVideoLoadError] = useState(false);
  const [videoObjectUrl, setVideoObjectUrl] = useState<string | null>(null);
  const [isVideoLoading, setIsVideoLoading] = useState(false);
  const [rotationByItem, setRotationByItem] = useState<Record<string, number>>({});

  const item =
    currentIndex >= 0 && currentIndex < items.length
      ? items[currentIndex]
      : null;

  useEffect(() => {
    setVideoLoadError(false);
  }, [item?.id, item?.mediaSrc, item?.mediaKind]);

  const videoMimeType = useMemo(() => {
    if (!item) {
      return "video/mp4";
    }

    const normalized = item.mediaSrc.toLowerCase();
    if (normalized.includes(".mov")) {
      return "video/quicktime";
    }

    if (normalized.includes(".webm")) {
      return "video/webm";
    }

    return "video/mp4";
  }, [item]);

  useEffect(() => {
    if (!open || !item || item.mediaKind !== "video") {
      setVideoObjectUrl((previous) => {
        if (previous) {
          URL.revokeObjectURL(previous);
        }
        return null;
      });
      setIsVideoLoading(false);
      return;
    }

    let isCancelled = false;
    let localObjectUrl: string | null = null;

    const loadVideoAsObjectUrl = async () => {
      try {
        setIsVideoLoading(true);
        setVideoLoadError(false);

        const response = await fetch(item.mediaSrc);
        if (!response.ok) {
          throw new Error(`video fetch failed with status ${response.status}`);
        }

        const blob = await response.blob();
        const typedBlob =
          blob.type && blob.type.length > 0
            ? blob
            : new Blob([blob], { type: videoMimeType });
        localObjectUrl = URL.createObjectURL(typedBlob);

        if (isCancelled) {
          URL.revokeObjectURL(localObjectUrl);
          return;
        }

        setVideoObjectUrl((previous) => {
          if (previous) {
            URL.revokeObjectURL(previous);
          }
          return localObjectUrl;
        });
      } catch (error) {
        console.error("[viewer] Failed to load video for modal playback", {
          mediaSrc: item.mediaSrc,
          error,
        });
        if (!isCancelled) {
          setVideoLoadError(true);
        }
      } finally {
        if (!isCancelled) {
          setIsVideoLoading(false);
        }
      }
    };

    void loadVideoAsObjectUrl();

    return () => {
      isCancelled = true;
      if (localObjectUrl) {
        URL.revokeObjectURL(localObjectUrl);
      }
    };
  }, [open, item, videoMimeType]);

  useEffect(() => {
    if (!open || !item) {
      return;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "q" || event.key === "Q") {
        event.preventDefault();
        setRotationByItem((previous) => ({
          ...previous,
          [item.id]: ((previous[item.id] ?? 0) + 270) % 360,
        }));
        return;
      }

      if (event.key === "e" || event.key === "E") {
        event.preventDefault();
        setRotationByItem((previous) => ({
          ...previous,
          [item.id]: ((previous[item.id] ?? 0) + 90) % 360,
        }));
      }
    };

    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open, item]);

  if (!open || items.length === 0 || !item) {
    return null;
  }
  const isFirst = currentIndex === 0;
  const isLast = currentIndex === items.length - 1;
  const currentRotation = rotationByItem[item.id] ?? 0;

  const rotateCurrentLeft = () => {
    setRotationByItem((previous) => ({
      ...previous,
      [item.id]: ((previous[item.id] ?? 0) + 270) % 360,
    }));
  };

  const rotateCurrentRight = () => {
    setRotationByItem((previous) => ({
      ...previous,
      [item.id]: ((previous[item.id] ?? 0) + 90) % 360,
    }));
  };

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
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="icon"
              className="h-9 w-9 border-white/20 bg-black/30 text-white hover:bg-black/50"
              onClick={rotateCurrentLeft}
              aria-label={t("viewer.modal.rotateLeft")}
            >
              <RotateCcw className="h-4 w-4" />
            </Button>

            <Button
              type="button"
              variant="outline"
              size="icon"
              className="h-9 w-9 border-white/20 bg-black/30 text-white hover:bg-black/50"
              onClick={rotateCurrentRight}
              aria-label={t("viewer.modal.rotateRight")}
            >
              <RotateCw className="h-4 w-4" />
            </Button>

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
          </div>
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
              <div className="flex h-full w-full flex-col items-center justify-center gap-3">
                {isVideoLoading ? (
                  <p className="text-xs text-white/85">{t("viewer.modal.videoLoading")}</p>
                ) : null}

                <video
                  key={videoObjectUrl ?? item.mediaSrc}
                  className="max-h-full max-w-full rounded-lg object-contain"
                  style={{ transform: `rotate(${currentRotation}deg)` }}
                  controls
                  autoPlay
                  muted
                  playsInline
                  preload="metadata"
                  onError={() => {
                    setVideoLoadError(true);
                    setIsVideoLoading(false);
                  }}
                >
                  <source src={videoObjectUrl ?? item.mediaSrc} type={videoMimeType} />
                </video>

                {videoLoadError ? (
                  <p className="max-w-3xl text-center text-xs text-white/85">
                    {t("viewer.modal.videoUnsupported")}
                  </p>
                ) : null}
              </div>
            ) : (
              <img
                src={item.mediaSrc}
                alt={t("viewer.modal.imageAlt", { id: item.id })}
                className="max-h-full max-w-full rounded-lg object-contain"
                style={{ transform: `rotate(${currentRotation}deg)` }}
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

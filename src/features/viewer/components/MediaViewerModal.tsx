import {
  X,
  ChevronLeft,
  ChevronRight,
  RotateCcw,
  RotateCw,
  Volume2,
  VolumeX,
  Maximize,
  Minimize,
  Info,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";

import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";
import type { ViewerMediaKind } from "@/lib/memories-api";
import { MediaMetadataModal } from "@/features/viewer/components/MediaMetadataModal";

type MediaViewerModalItem = {
  id: string;
  mediaSrc: string;
  mediaKind: ViewerMediaKind;
  dateTaken: string;
  location?: string;
  rawLocation?: string;
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
  const [isSoundEnabled, setIsSoundEnabled] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [rotationByItem, setRotationByItem] = useState<Record<string, number>>({});
  const [isMetadataOpen, setIsMetadataOpen] = useState(false);
  const mediaContainerRef = useRef<HTMLDivElement | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);

  const syncFullscreenState = () => {
    const webkitDocument = document as Document & {
      webkitFullscreenElement?: Element | null;
    };
    const fullscreenElement =
      document.fullscreenElement ?? webkitDocument.webkitFullscreenElement ?? null;

    if (!fullscreenElement) {
      setIsFullscreen(false);
      return;
    }

    const mediaContainer = mediaContainerRef.current;
    const videoElement = videoRef.current;
    const isManagedFullscreen =
      (mediaContainer !== null &&
        (fullscreenElement === mediaContainer || mediaContainer.contains(fullscreenElement))) ||
      (videoElement !== null &&
        (fullscreenElement === videoElement ||
          videoElement.contains(fullscreenElement) ||
          (fullscreenElement instanceof HTMLElement && fullscreenElement.contains(videoElement))));

    setIsFullscreen(isManagedFullscreen);
  };

  const item =
    currentIndex >= 0 && currentIndex < items.length ? items[currentIndex] : null;

  useEffect(() => {
    setVideoLoadError(false);
    setIsMetadataOpen(false);
  }, [item?.id, item?.mediaSrc, item?.mediaKind]);

  useEffect(() => {
    if (!open) {
      setIsSoundEnabled(false);
      setIsFullscreen(false);
      return;
    }

    syncFullscreenState();
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    syncFullscreenState();
  }, [open, item?.id]);

  useEffect(() => {
    const onFullscreenChange = () => {
      syncFullscreenState();
    };

    document.addEventListener("fullscreenchange", onFullscreenChange);
    document.addEventListener("webkitfullscreenchange", onFullscreenChange as EventListener);

    return () => {
      document.removeEventListener("fullscreenchange", onFullscreenChange);
      document.removeEventListener("webkitfullscreenchange", onFullscreenChange as EventListener);
    };
  }, []);

  useEffect(() => {
    if (!open) {
      return;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }

      window.setTimeout(() => {
        syncFullscreenState();
      }, 0);
    };

    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  useEffect(() => {
    if (!open || !item || item.mediaKind !== "video") {
      return;
    }

    const video = videoRef.current;
    if (!video) {
      return;
    }

    const syncFromVideo = () => {
      const hasSound = !video.muted && video.volume > 0;
      setIsSoundEnabled(hasSound);
    };

    const syncFullscreenFromVideo = () => {
      syncFullscreenState();
    };

    video.addEventListener("volumechange", syncFromVideo);
    video.addEventListener("webkitbeginfullscreen", syncFullscreenFromVideo as EventListener);
    video.addEventListener("webkitendfullscreen", syncFullscreenFromVideo as EventListener);
    syncFromVideo();

    return () => {
      video.removeEventListener("volumechange", syncFromVideo);
      video.removeEventListener("webkitbeginfullscreen", syncFullscreenFromVideo as EventListener);
      video.removeEventListener("webkitendfullscreen", syncFullscreenFromVideo as EventListener);
    };
  }, [open, item?.id, item?.mediaKind]);

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
  const isQuarterTurn = currentRotation % 180 !== 0;
  const fullscreenInset = "5rem";

  const mediaStyle: CSSProperties = {
    transform: `rotate(${currentRotation}deg)`,
    transformOrigin: "center center",
    maxWidth: isFullscreen
      ? isQuarterTurn
        ? `calc(100dvh - ${fullscreenInset})`
        : `calc(100dvw - ${fullscreenInset})`
      : isQuarterTurn
        ? "calc(100dvh - 10rem)"
        : "100%",
    maxHeight: isFullscreen
      ? isQuarterTurn
        ? `calc(100dvw - ${fullscreenInset})`
        : `calc(100dvh - ${fullscreenInset})`
      : isQuarterTurn
        ? "calc(100dvw - 7rem)"
        : "100%",
  };

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

  const toggleSound = () => {
    const video = videoRef.current;
    if (!video) {
      setIsSoundEnabled((previous) => !previous);
      return;
    }

    const nextHasSound = video.muted || video.volume <= 0;
    if (nextHasSound && video.volume <= 0) {
      video.volume = 1;
    }
    video.muted = !nextHasSound;
    setIsSoundEnabled(nextHasSound);
  };

  const exitFullscreen = async () => {
    if (!document.fullscreenElement) {
      setIsFullscreen(false);
      return;
    }

    try {
      await document.exitFullscreen();
    } catch (error) {
      console.error("[viewer] Failed to exit fullscreen", { error });
    } finally {
      setIsFullscreen(false);
    }
  };

  const toggleFullscreen = async () => {
    if (document.fullscreenElement) {
      await exitFullscreen();
      return;
    }

    const fullscreenTarget = mediaContainerRef.current;
    try {
      await fullscreenTarget?.requestFullscreen();
      setIsFullscreen(true);
    } catch (error) {
      console.error("[viewer] Failed to enter fullscreen", { error });
      setIsFullscreen(false);
    }
  };

  return (
    <>
    <div
      className="fixed inset-0 z-100 bg-black/85 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label={t("viewer.modal.title")}
    >
      <div className="flex h-full w-full flex-col">
        <header className="flex items-center justify-between px-4 py-3">
          <p className="text-sm text-white/90">
            {t("viewer.modal.counter", {
              current: currentIndex + 1,
              total: items.length,
            })}
          </p>

          <div className="flex items-center gap-2">
            {item.mediaKind === "video" ? (
              <Button
                type="button"
                variant="outline"
                size="icon"
                className="h-9 w-9 border-white/20 bg-black/30 text-white hover:bg-black/50"
                onClick={toggleSound}
                aria-label={
                  isSoundEnabled
                    ? t("viewer.modal.soundDisable")
                    : t("viewer.modal.soundEnable")
                }
              >
                {isSoundEnabled ? <Volume2 className="h-4 w-4" /> : <VolumeX className="h-4 w-4" />}
              </Button>
            ) : null}

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
              onClick={() => {
                void toggleFullscreen();
              }}
              aria-label={
                isFullscreen
                  ? t("viewer.modal.exitFullscreen")
                  : t("viewer.modal.enterFullscreen")
              }
            >
              {isFullscreen ? <Minimize className="h-4 w-4" /> : <Maximize className="h-4 w-4" />}
            </Button>

              <Button
                type="button"
                variant="outline"
                size="icon"
                className="h-9 w-9 border-white/20 bg-black/30 text-white hover:bg-black/50"
                onClick={() => setIsMetadataOpen(true)}
                aria-label={t("viewer.modal.showMetadata")}
              >
                <Info className="h-4 w-4" />
              </Button>

            <Button
              type="button"
              variant="outline"
              size="icon"
              className="h-9 w-9 border-white/20 bg-black/30 text-white hover:bg-black/50"
              onClick={() => {
                if (document.fullscreenElement) {
                  void exitFullscreen();
                  return;
                }

                onClose();
              }}
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

          <div
            ref={mediaContainerRef}
            className={`relative flex h-full w-full items-center justify-center overflow-hidden ${
              isFullscreen ? "bg-black" : "bg-transparent"
            }`}
          >
            {isFullscreen ? (
              <>
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="absolute left-4 z-20 h-10 w-10 border-white/20 bg-black/40 text-white hover:bg-black/60 disabled:opacity-30"
                  onClick={onPrevious}
                  disabled={isFirst}
                  aria-label={t("viewer.modal.previous")}
                >
                  <ChevronLeft className="h-5 w-5" />
                </Button>

                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="absolute right-4 z-20 h-10 w-10 border-white/20 bg-black/40 text-white hover:bg-black/60 disabled:opacity-30"
                  onClick={onNext}
                  disabled={isLast}
                  aria-label={t("viewer.modal.next")}
                >
                  <ChevronRight className="h-5 w-5" />
                </Button>
              </>
            ) : null}

            {isFullscreen ? (
              <div className="absolute right-4 top-4 z-20 flex items-center gap-2">
                {item.mediaKind === "video" ? (
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    className="h-10 w-10 border-white/20 bg-black/40 text-white hover:bg-black/60"
                    onClick={toggleSound}
                    aria-label={
                      isSoundEnabled
                        ? t("viewer.modal.soundDisable")
                        : t("viewer.modal.soundEnable")
                    }
                  >
                    {isSoundEnabled ? <Volume2 className="h-4 w-4" /> : <VolumeX className="h-4 w-4" />}
                  </Button>
                ) : null}

                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="h-10 w-10 border-white/20 bg-black/40 text-white hover:bg-black/60"
                  onClick={rotateCurrentLeft}
                  aria-label={t("viewer.modal.rotateLeft")}
                >
                  <RotateCcw className="h-4 w-4" />
                </Button>

                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="h-10 w-10 border-white/20 bg-black/40 text-white hover:bg-black/60"
                  onClick={rotateCurrentRight}
                  aria-label={t("viewer.modal.rotateRight")}
                >
                  <RotateCw className="h-4 w-4" />
                </Button>

                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="h-10 w-10 border-white/20 bg-black/40 text-white hover:bg-black/60"
                  onClick={() => setIsMetadataOpen(true)}
                  aria-label={t("viewer.modal.showMetadata")}
                >
                  <Info className="h-4 w-4" />
                </Button>

                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="h-10 w-10 border-white/20 bg-black/40 text-white hover:bg-black/60"
                  onClick={() => {
                    void exitFullscreen();
                  }}
                  aria-label={t("viewer.modal.close")}
                >
                  <X className="h-4 w-4" />
                </Button>
              </div>
            ) : null}

            {item.mediaKind === "video" ? (
              <div className="flex h-full w-full flex-col items-center justify-center gap-3">
                {isVideoLoading ? (
                  <p className="text-xs text-white/85">{t("viewer.modal.videoLoading")}</p>
                ) : null}

                <video
                  ref={videoRef}
                  key={videoObjectUrl ?? item.mediaSrc}
                  className="h-auto max-h-full w-auto max-w-full rounded-lg object-contain"
                  style={mediaStyle}
                  controls
                  autoPlay
                  muted={!isSoundEnabled}
                  playsInline
                  preload="metadata"
                  onVolumeChange={(event) => {
                    const target = event.currentTarget;
                    const hasSound = !target.muted && target.volume > 0;
                    setIsSoundEnabled(hasSound);
                  }}
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
                className="h-auto max-h-full w-auto max-w-full rounded-lg object-contain"
                style={mediaStyle}
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

    <MediaMetadataModal
      open={isMetadataOpen}
      onClose={() => setIsMetadataOpen(false)}
      item={{
        id: item.id,
        dateTaken: item.dateTaken,
        mediaKind: item.mediaKind,
        location: item.location,
        rawLocation: item.rawLocation,
      }}
    />
    </>
  );
}

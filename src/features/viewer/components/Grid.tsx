import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

import { Film, ImageIcon } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
import {
  formatViewerFullDate,
  formatViewerShortDate,
} from "@/features/viewer/viewer-dates";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { ViewerMediaKind } from "@/lib/memories-api";

export type ThumbnailItem = {
  id: string;
  src?: string;
  dateTaken: string;
  mediaKind: ViewerMediaKind;
  location?: string;
  rawLocation?: string;
  mediaIndex: number;
};

export type GridStickyHeader = {
  variant: "dated" | "unknown";
  yearLabel: string | null;
  monthLabel: string | null;
};

export type GridTimelineRow =
  | {
      kind: "year" | "month" | "unknown";
      id: string;
      label: string;
      stickyHeader: GridStickyHeader;
    }
  | {
      kind: "media";
      id: string;
      items: ThumbnailItem[];
      stickyHeader: GridStickyHeader;
    };

type GridProps = {
  rows: GridTimelineRow[];
  onItemSelect?: (index: number) => void;
};

type TimelineRailMarker = {
  id: string;
  kind: "year" | "month" | "unknown";
  label: string;
  rowIndex: number;
  positionRatio: number;
  yearLabel: string | null;
  monthLabel: string | null;
  stickyHeader: GridStickyHeader;
};

export const GRID_COLUMNS = 4;
const ESTIMATED_MEDIA_ROW_HEIGHT = 220;
const ESTIMATED_YEAR_ROW_HEIGHT = 60;
const ESTIMATED_MONTH_ROW_HEIGHT = 52;
const RAIL_NEARBY_YEAR_RANGE = 1;

function getHeaderDisplayLabel(header: GridStickyHeader | null): string | null {
  if (!header) {
    return null;
  }

  if (header.monthLabel && header.yearLabel) {
    return `${header.monthLabel} ${header.yearLabel}`;
  }

  return header.monthLabel ?? header.yearLabel ?? null;
}

function resolveActiveSection(
  rows: GridTimelineRow[],
  virtualRows: VirtualItem[],
  scrollTop: number,
): { rowIndex: number; header: GridStickyHeader | null } {
  if (rows.length === 0) {
    return { rowIndex: 0, header: null };
  }

  const targetOffset = scrollTop + 1;
  const topVisibleRow =
    virtualRows.find((virtualRow) => virtualRow.end > targetOffset) ?? virtualRows[0] ?? null;

  const rowIndex = topVisibleRow?.index ?? 0;
  const candidateRow = rows[rowIndex];
  if (!candidateRow) {
    return { rowIndex, header: null };
  }

  if (candidateRow.stickyHeader.monthLabel) {
    return { rowIndex, header: candidateRow.stickyHeader };
  }

  const nextRow = rows[rowIndex + 1];
  if (
    nextRow &&
    nextRow.stickyHeader.variant === candidateRow.stickyHeader.variant &&
    nextRow.stickyHeader.yearLabel === candidateRow.stickyHeader.yearLabel &&
    nextRow.stickyHeader.monthLabel
  ) {
    return { rowIndex, header: nextRow.stickyHeader };
  }

  return { rowIndex, header: candidateRow.stickyHeader };
}

function buildRailMarkers(rows: GridTimelineRow[]): TimelineRailMarker[] {
  if (rows.length === 0) {
    return [];
  }

  const lastRowIndex = Math.max(rows.length - 1, 1);

  return rows.flatMap((row, rowIndex) => {
    if (row.kind === "media") {
      return [];
    }

    return [
      {
        id: row.id,
        kind: row.kind,
        label: row.label,
        rowIndex,
        positionRatio: rowIndex / lastRowIndex,
        yearLabel: row.stickyHeader.yearLabel,
        monthLabel: row.stickyHeader.monthLabel,
        stickyHeader: row.stickyHeader,
      },
    ];
  });
}

function estimateRowHeight(row: GridTimelineRow | undefined): number {
  if (!row) {
    return ESTIMATED_MEDIA_ROW_HEIGHT;
  }

  switch (row.kind) {
    case "media":
      return ESTIMATED_MEDIA_ROW_HEIGHT;
    case "year":
      return ESTIMATED_YEAR_ROW_HEIGHT;
    case "month":
    case "unknown":
      return ESTIMATED_MONTH_ROW_HEIGHT;
    default:
      return ESTIMATED_MEDIA_ROW_HEIGHT;
  }
}

function StickyTimelineHeader({ header }: { header: GridStickyHeader }) {
  const labels = [header.yearLabel, header.monthLabel].filter(
    (label): label is string => Boolean(label),
  );

  if (labels.length === 0) {
    return null;
  }

  return (
    <div className="pointer-events-none sticky top-0 z-20 px-3 pt-3">
      <div className="inline-flex max-w-full flex-col gap-1 rounded-xl border border-border/80 bg-background/92 px-3 py-2 shadow-lg backdrop-blur-sm">
        {labels.map((label, index) => (
          <span
            key={`${header.variant}-${label}`}
            className={cn(
              "truncate leading-none",
              index === 0
                ? "text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground"
                : "text-sm font-semibold text-foreground",
            )}
          >
            {label}
          </span>
        ))}
      </div>
    </div>
  );
}

function TimelineRail({
  markers,
  activeMarker,
  bubbleMarker,
  activeHeader,
  isScrubbing,
  progressRatio,
  onJumpToMarker,
  onPointerDown,
  railTrackRef,
}: {
  markers: TimelineRailMarker[];
  activeMarker: TimelineRailMarker | null;
  bubbleMarker: TimelineRailMarker | null;
  activeHeader: GridStickyHeader | null;
  isScrubbing: boolean;
  progressRatio: number;
  onJumpToMarker: (marker: TimelineRailMarker) => void;
  onPointerDown: (event: ReactPointerEvent<HTMLDivElement>) => void;
  railTrackRef: React.RefObject<HTMLDivElement | null>;
}) {
  const { t } = useI18n();

  const yearMarkers = markers.filter((marker) => marker.kind === "year");
  const activeYearLabel = activeHeader?.variant === "dated" ? activeHeader.yearLabel : null;
  const activeYearIndex = yearMarkers.findIndex((marker) => marker.yearLabel === activeYearLabel);

  const visibleYearLabels = new Set(
    yearMarkers
      .filter((_, index) => {
        if (activeYearIndex < 0) {
          return false;
        }

        return Math.abs(index - activeYearIndex) <= RAIL_NEARBY_YEAR_RANGE;
      })
      .map((marker) => marker.yearLabel)
      .filter((label): label is string => Boolean(label)),
  );

  const visibleMarkers = markers.filter((marker) => {
    if (marker.kind === "year" || marker.kind === "unknown") {
      return true;
    }

    return marker.yearLabel !== null && visibleYearLabels.has(marker.yearLabel);
  });

  const bubbleLabel = getHeaderDisplayLabel(bubbleMarker?.stickyHeader ?? activeHeader);
  const indicatorRatio = bubbleMarker?.positionRatio ?? progressRatio;

  return (
    <div className="pointer-events-none absolute inset-y-0 right-0 z-30 flex w-16 items-center justify-center pr-2">
      <div ref={railTrackRef} className="relative h-[calc(100%-1.5rem)] w-full">
        {bubbleLabel ? (
          <div
            className={cn(
              "pointer-events-none absolute right-8 z-10 -translate-y-1/2 rounded-full border border-border/80 bg-background/95 px-2.5 py-1 text-[11px] font-medium shadow-md backdrop-blur-sm transition-opacity",
              isScrubbing ? "opacity-100" : "opacity-90",
            )}
            style={{
              top: `${indicatorRatio * 100}%`,
            }}
          >
            {bubbleLabel}
          </div>
        ) : null}

        <div
          className="pointer-events-auto absolute inset-y-0 right-2 flex w-10 touch-none justify-center"
          onPointerDown={onPointerDown}
          aria-label={t("viewer.timeline.rail")}
        >
          <div className="relative h-full w-full">
            <div className="absolute inset-y-1/2 left-1/2 w-px -translate-x-1/2 -translate-y-1/2 rounded-full bg-border" />
            <div
              className="pointer-events-none absolute left-1/2 h-8 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary/70 shadow-[0_0_0_3px_rgba(59,130,246,0.10)] transition-transform"
              style={{ top: `${indicatorRatio * 100}%` }}
            />

            {visibleMarkers.map((marker) => {
              const isActive = activeMarker?.id === marker.id;
              const commonStyle = {
                top: `${marker.positionRatio * 100}%`,
              };

              if (marker.kind === "year") {
                return (
                  <button
                    key={marker.id}
                    type="button"
                    className={cn(
                      "absolute right-4 -translate-y-1/2 rounded-full border px-2 py-0.5 text-[10px] font-semibold tracking-[0.14em] shadow-sm transition-all",
                      isActive
                        ? "border-primary bg-primary text-primary-foreground"
                        : "border-border/80 bg-background/92 text-muted-foreground hover:border-primary/40 hover:text-foreground",
                    )}
                    style={commonStyle}
                    onPointerDown={(event) => {
                      event.stopPropagation();
                    }}
                    onClick={() => onJumpToMarker(marker)}
                    aria-label={t("viewer.timeline.jumpToSection", { label: marker.label })}
                    title={marker.label}
                  >
                    {marker.label}
                  </button>
                );
              }

              if (marker.kind === "unknown") {
                return (
                  <button
                    key={marker.id}
                    type="button"
                    className={cn(
                      "absolute left-1/2 flex h-3 w-3 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full border text-[8px] font-semibold transition-all",
                      isActive
                        ? "border-primary bg-primary text-primary-foreground"
                        : "border-border/90 bg-background/90 text-muted-foreground hover:border-primary/40 hover:text-foreground",
                    )}
                    style={commonStyle}
                    onPointerDown={(event) => {
                      event.stopPropagation();
                    }}
                    onClick={() => onJumpToMarker(marker)}
                    aria-label={t("viewer.timeline.jumpToSection", { label: marker.label })}
                    title={marker.label}
                  >
                    ?
                  </button>
                );
              }

              return (
                <button
                  key={marker.id}
                  type="button"
                  className={cn(
                    "absolute left-1/2 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border transition-all",
                    isActive
                      ? "scale-125 border-primary bg-primary shadow-[0_0_0_3px_rgba(59,130,246,0.14)]"
                      : "border-border/80 bg-background/95 hover:scale-110 hover:border-primary/40",
                  )}
                  style={commonStyle}
                  onPointerDown={(event) => {
                    event.stopPropagation();
                  }}
                  onClick={() => onJumpToMarker(marker)}
                  aria-label={t("viewer.timeline.jumpToSection", { label: marker.label })}
                  title={marker.label}
                />
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

export function Grid({ rows, onItemSelect }: GridProps) {
  const { t, resolvedLocale } = useI18n();
  const parentRef = useRef<HTMLDivElement>(null);
  const railRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [isScrubbing, setIsScrubbing] = useState(false);
  const [scrubMarker, setScrubMarker] = useState<TimelineRailMarker | null>(null);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => estimateRowHeight(rows[index]),
    overscan: 6,
  });

  const virtualRows = rowVirtualizer.getVirtualItems();
  const activeSection = useMemo(
    () => resolveActiveSection(rows, virtualRows, scrollTop),
    [rows, scrollTop, virtualRows],
  );
  const activeStickyHeader = activeSection.header;
  const totalSize = rowVirtualizer.getTotalSize();
  const viewportHeight = parentRef.current?.clientHeight ?? 0;
  const maxScrollTop = Math.max(totalSize - viewportHeight, 1);
  const scrollProgressRatio = Math.min(scrollTop / maxScrollTop, 1);

  const railMarkers = useMemo(() => buildRailMarkers(rows), [rows]);
  const activeMarker = useMemo(() => {
    if (!activeStickyHeader) {
      return null;
    }

    if (activeStickyHeader.variant === "unknown") {
      return railMarkers.find((marker) => marker.kind === "unknown") ?? null;
    }

    if (activeStickyHeader.monthLabel) {
      return (
        railMarkers.find(
          (marker) =>
            marker.kind === "month" &&
            marker.yearLabel === activeStickyHeader.yearLabel &&
            marker.monthLabel === activeStickyHeader.monthLabel,
        ) ?? null
      );
    }

    return (
      railMarkers.find(
        (marker) => marker.kind === "year" && marker.yearLabel === activeStickyHeader.yearLabel,
      ) ?? null
    );
  }, [activeStickyHeader, railMarkers]);

  const scrollToMarker = useCallback(
    (marker: TimelineRailMarker) => {
      rowVirtualizer.scrollToIndex(marker.rowIndex, { align: "start" });
    },
    [rowVirtualizer],
  );

  const updateScrubMarker = useCallback(
    (clientY: number) => {
      const track = railRef.current;
      if (!track || railMarkers.length === 0) {
        return;
      }

      const rect = track.getBoundingClientRect();
      if (rect.height <= 0) {
        return;
      }

      const nextRatio = Math.min(Math.max((clientY - rect.top) / rect.height, 0), 1);
      const nextMarker = railMarkers.reduce<TimelineRailMarker | null>((closest, marker) => {
        if (!closest) {
          return marker;
        }

        return Math.abs(marker.positionRatio - nextRatio) < Math.abs(closest.positionRatio - nextRatio)
          ? marker
          : closest;
      }, null);

      if (!nextMarker) {
        return;
      }

      setScrubMarker((current) => {
        if (current?.id === nextMarker.id) {
          return current;
        }

        scrollToMarker(nextMarker);
        return nextMarker;
      });
    },
    [railMarkers, scrollToMarker],
  );

  useEffect(() => {
    if (!isScrubbing) {
      return;
    }

    const handlePointerMove = (event: PointerEvent) => {
      event.preventDefault();
      updateScrubMarker(event.clientY);
    };

    const handlePointerUp = () => {
      setIsScrubbing(false);
      setScrubMarker(null);
    };

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerUp);

    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerUp);
    };
  }, [isScrubbing, updateScrubMarker]);

  const handleRailPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      event.preventDefault();
      setIsScrubbing(true);
      updateScrubMarker(event.clientY);
    },
    [updateScrubMarker],
  );

  return (
    <TooltipProvider delayDuration={400}>
      <div className="relative h-full min-h-80 rounded-md border border-border">
      <div
        ref={parentRef}
        className="relative h-full overflow-auto rounded-md"
        onScroll={(event) => {
          setScrollTop(event.currentTarget.scrollTop);
        }}
      >
        {activeStickyHeader ? <StickyTimelineHeader header={activeStickyHeader} /> : null}
        <div
          className="relative w-full"
          style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
        >
          {virtualRows.map((virtualRow) => {
            const row = rows[virtualRow.index];
            if (!row) {
              return null;
            }

            if (row.kind !== "media") {
              return (
                <div
                  key={virtualRow.key}
                  data-index={virtualRow.index}
                  ref={rowVirtualizer.measureElement}
                  className="absolute left-0 top-0 w-full px-3 py-2 pr-16"
                  style={{ transform: `translateY(${virtualRow.start}px)` }}
                >
                  <div
                    className={cn(
                      "rounded-xl border px-4 py-3 shadow-xs",
                      row.kind === "year"
                        ? "border-border/80 bg-muted/60"
                        : "border-border/60 bg-background/80",
                    )}
                  >
                    <span
                      className={cn(
                        "block truncate",
                        row.kind === "year"
                          ? "text-sm font-semibold uppercase tracking-[0.2em] text-foreground"
                          : "text-sm font-medium text-muted-foreground",
                      )}
                    >
                      {row.label}
                    </span>
                  </div>
                </div>
              );
            }

            return (
              <div
                key={virtualRow.key}
                data-index={virtualRow.index}
                ref={rowVirtualizer.measureElement}
                className="absolute left-0 top-0 grid w-full grid-cols-2 gap-2 p-2 pr-16 sm:grid-cols-3 md:grid-cols-4"
                style={{ transform: `translateY(${virtualRow.start}px)` }}
              >
                {row.items.map((item) => {
                  const shortDate = formatViewerShortDate(item.dateTaken, resolvedLocale);
                  const fullDate = formatViewerFullDate(item.dateTaken, resolvedLocale);

                  const typeLabel =
                    item.mediaKind === "video"
                      ? t("viewer.metadata.type.video")
                      : t("viewer.metadata.type.image");

                  return (
                    <Tooltip key={item.id}>
                      <TooltipTrigger asChild>
                        <div className="relative aspect-9/16 overflow-hidden rounded-md border border-border bg-background">
                          {item.src ? (
                            <button
                              type="button"
                              className="block h-full w-full"
                              onClick={() => onItemSelect?.(item.mediaIndex)}
                              aria-label={t("viewer.grid.openMedia", { id: item.id })}
                              id={`viewer-thumb-${item.id}`}
                            >
                              <img
                                src={item.src}
                                alt={t("viewer.grid.thumbnailAlt", { id: item.id })}
                                loading="lazy"
                                className="block h-full w-full bg-background object-contain"
                                onError={() => {
                                  console.error("[viewer] Thumbnail failed to load", {
                                    id: item.id,
                                    src: item.src,
                                  });
                                }}
                              />
                            </button>
                          ) : (
                            <div className="flex h-full w-full items-center justify-center text-xs text-muted-foreground">
                              {item.id}
                            </div>
                          )}
                          {item.src ? (
                            <div className="pointer-events-none absolute inset-x-0 bottom-0 flex items-end justify-between bg-linear-to-t from-black/50 to-transparent px-2 pb-1.5 pt-6">
                              {item.mediaKind === "video" ? (
                                <Film className="h-4 w-4 shrink-0 text-white/80" style={{ filter: "drop-shadow(0 1px 2px rgba(0,0,0,0.8))" }} />
                              ) : (
                                <ImageIcon className="h-4 w-4 shrink-0 text-white/80" style={{ filter: "drop-shadow(0 1px 2px rgba(0,0,0,0.8))" }} />
                              )}
                              {shortDate ? (
                                <span
                                  className="truncate pl-1 text-[10px] leading-none tabular-nums text-white/80"
                                  style={{ textShadow: "0 1px 2px rgba(0,0,0,0.8)" }}
                                >
                                  {shortDate}
                                </span>
                              ) : null}
                            </div>
                          ) : null}
                        </div>
                      </TooltipTrigger>
                      <TooltipContent
                        sideOffset={6}
                        variant="popover"
                        className="max-w-52"
                      >
                            <div className="flex items-start gap-1.5">
                              <span className="shrink-0 text-muted-foreground">{t("viewer.grid.tooltip.date")}:</span>
                              <span>{fullDate}</span>
                            </div>
                            <div className="flex items-start gap-1.5">
                              <span className="shrink-0 text-muted-foreground">{t("viewer.grid.tooltip.type")}:</span>
                              <span>{typeLabel}</span>
                            </div>
                            {item.location ? (
                              <div className="flex items-start gap-1.5">
                                <span className="shrink-0 text-muted-foreground">{t("viewer.grid.tooltip.location")}:</span>
                                <span>{item.location}</span>
                              </div>
                            ) : null}
                      </TooltipContent>
                    </Tooltip>
                  );
                })}
              </div>
            );
          })}
        </div>
      </div>
        {railMarkers.length > 0 ? (
          <TimelineRail
            markers={railMarkers}
            activeMarker={activeMarker}
            bubbleMarker={scrubMarker}
            activeHeader={activeStickyHeader}
            isScrubbing={isScrubbing}
            progressRatio={scrollProgressRatio}
            onJumpToMarker={scrollToMarker}
            onPointerDown={handleRailPointerDown}
            railTrackRef={railRef}
          />
        ) : null}
      </div>
    </TooltipProvider>
  );
}

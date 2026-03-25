import { useMemo, useRef } from "react";

import { Film, ImageIcon } from "lucide-react";
import { Tooltip } from "radix-ui";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useI18n } from "@/lib/i18n";
import type { ViewerMediaKind } from "@/lib/memories-api";

type ThumbnailItem = {
  id: string;
  src?: string;
  dateTaken: string;
  mediaKind: ViewerMediaKind;
  location?: string;
};

type GridProps = {
  items: ThumbnailItem[];
  onItemSelect?: (index: number) => void;
};

const GRID_COLUMNS = 4;
const ESTIMATED_ROW_HEIGHT = 220;

export function Grid({ items, onItemSelect }: GridProps) {
  const { t, resolvedLocale } = useI18n();
  const parentRef = useRef<HTMLDivElement>(null);

  const rows = useMemo(() => {
    const rowCount = Math.ceil(items.length / GRID_COLUMNS);
    return Array.from({ length: rowCount }, (_, rowIndex) => {
      const start = rowIndex * GRID_COLUMNS;
      return items.slice(start, start + GRID_COLUMNS);
    });
  }, [items]);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ESTIMATED_ROW_HEIGHT,
    overscan: 6,
  });

  return (
    <Tooltip.Provider delayDuration={400}>
      <div
        ref={parentRef}
        className="relative h-full min-h-80 overflow-auto rounded-md border border-border"
      >
        <div
          className="relative w-full"
          style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
        >
          {rowVirtualizer.getVirtualItems().map((virtualRow) => {
            const rowItems = rows[virtualRow.index] ?? [];

            return (
              <div
                key={virtualRow.key}
                ref={rowVirtualizer.measureElement}
                className="absolute left-0 top-0 grid w-full grid-cols-2 gap-2 p-2 sm:grid-cols-4"
                style={{ transform: `translateY(${virtualRow.start}px)` }}
              >
                {rowItems.map((item, itemIndex) => {
                  const absoluteIndex = virtualRow.index * GRID_COLUMNS + itemIndex;
                  const date = new Date(item.dateTaken);
                  const isValidDate = !isNaN(date.getTime());

                  const shortDate = isValidDate
                    ? new Intl.DateTimeFormat(resolvedLocale, {
                        day: "2-digit",
                        month: "2-digit",
                        year: "numeric",
                      }).format(date)
                    : "";

                  const fullDate = isValidDate
                    ? new Intl.DateTimeFormat(resolvedLocale, {
                        day: "numeric",
                        month: "long",
                        year: "numeric",
                      }).format(date)
                    : item.dateTaken;

                  const typeLabel =
                    item.mediaKind === "video"
                      ? t("viewer.metadata.type.video")
                      : t("viewer.metadata.type.image");

                  return (
                    <Tooltip.Root key={item.id}>
                      <Tooltip.Trigger asChild>
                        <div className="relative aspect-9/16 overflow-hidden rounded-md border border-border bg-background">
                          {item.src ? (
                            <button
                              type="button"
                              className="block h-full w-full"
                              onClick={() => onItemSelect?.(absoluteIndex)}
                              aria-label={t("viewer.grid.openMedia", { id: item.id })}
                              id={`viewer-thumb-${item.id}`}
                            >
                              <img
                                src={item.src}
                                alt={t("viewer.grid.thumbnailAlt", { id: item.id })}
                                loading="lazy"
                                className="block h-full w-full scale-[1.01] bg-background object-cover"
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
                      </Tooltip.Trigger>
                      <Tooltip.Portal>
                        <Tooltip.Content
                          sideOffset={6}
                          className="z-200 max-w-50 rounded-md border border-border bg-background px-3 py-2 text-xs text-foreground shadow-md"
                        >
                          <div className="space-y-1">
                            <div className="flex gap-1.5">
                              <span className="text-muted-foreground">{t("viewer.grid.tooltip.date")}:</span>
                              <span>{fullDate}</span>
                            </div>
                            <div className="flex gap-1.5">
                              <span className="text-muted-foreground">{t("viewer.grid.tooltip.type")}:</span>
                              <span>{typeLabel}</span>
                            </div>
                            {item.location ? (
                              <div className="flex gap-1.5">
                                <span className="text-muted-foreground">{t("viewer.grid.tooltip.location")}:</span>
                                <span>{item.location}</span>
                              </div>
                            ) : null}
                          </div>
                          <Tooltip.Arrow className="fill-background" />
                        </Tooltip.Content>
                      </Tooltip.Portal>
                    </Tooltip.Root>
                  );
                })}
              </div>
            );
          })}
        </div>
      </div>
    </Tooltip.Provider>
  );
}

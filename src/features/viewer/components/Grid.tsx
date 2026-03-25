import { useMemo, useRef } from "react";

import { useVirtualizer } from "@tanstack/react-virtual";
import { useI18n } from "@/lib/i18n";

type ThumbnailItem = {
  id: string;
  src?: string;
};

type GridProps = {
  items: ThumbnailItem[];
  onItemSelect?: (index: number) => void;
};

const GRID_COLUMNS = 4;
const ESTIMATED_ROW_HEIGHT = 220;

export function Grid({ items, onItemSelect }: GridProps) {
  const { t } = useI18n();
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
    <div
      ref={parentRef}
      className="relative h-full min-h-[320px] overflow-auto rounded-md border border-border"
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

                return (
                <div
                  key={item.id}
                  className="aspect-[9/16] overflow-hidden rounded-md border border-border bg-background"
                >
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
                </div>
              );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

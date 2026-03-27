import { convertFileSrc } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  GRID_COLUMNS,
  Grid,
  type GridStickyHeader,
  type GridTimelineRow,
} from "@/features/viewer/components/Grid";
import { ViewerFilterBar } from "@/features/viewer/components/ViewerFilterBar";
import { MediaViewerModal } from "@/features/viewer/components/MediaViewerModal";
import {
  applyViewerFilters,
  DEFAULT_FILTER_STATE,
  extractFilterMeta,
  type ViewerFilterState,
} from "@/features/viewer/viewer-filters";
import {
  formatViewerMonthLabel,
  formatViewerYearLabel,
  getViewerYearMonth,
} from "@/features/viewer/viewer-dates";
import { useI18n } from "@/lib/i18n";
import {
  createViewerExportZip,
  getViewerItems,
  importViewerExportZip,
  type ViewerMediaKind,
} from "@/lib/memories-api";

type GridItem = {
  id: string;
  thumbnailSrc: string;
  mediaSrc: string;
  mediaKind: ViewerMediaKind;
  mediaFormat?: string;
  dateTaken: string;
  location?: string;
  rawLocation?: string;
};

type TimelineThumbnailItem = {
  id: string;
  src: string;
  dateTaken: string;
  mediaKind: ViewerMediaKind;
  location?: string;
  rawLocation?: string;
  mediaIndex: number;
};

export function ViewerPlaceholder() {
  const { t, resolvedLocale } = useI18n();
  const [items, setItems] = useState<GridItem[]>([]);
  const [filterState, setFilterState] = useState<ViewerFilterState>(() => ({
    ...DEFAULT_FILTER_STATE,
    mediaKinds: new Set(),
    mediaFormats: new Set(),
    timeSlots: new Set(),
    countries: new Set(),
  }));
  const [status, setStatus] = useState(t("viewer.status.loading"));
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const [isImporting, setIsImporting] = useState(false);
  const [isExporting, setIsExporting] = useState(false);

  const filterMeta = useMemo(() => extractFilterMeta(items), [items]);
  const filteredItems = useMemo(() => applyViewerFilters(items, filterState), [items, filterState]);

  const loadViewerItems = useCallback(async () => {
    try {
      const viewerRows = await getViewerItems(0, 5000);
      const mappedItems = viewerRows.map((row) => ({
        id: String(row.memoryItemId),
        thumbnailSrc: convertFileSrc(row.thumbnailPath, "asset"),
        mediaSrc: convertFileSrc(row.mediaPath, "asset"),
        mediaKind: row.mediaKind,
        mediaFormat: row.mediaFormat ?? undefined,
        dateTaken: row.dateTaken,
        location: row.location ?? undefined,
        rawLocation: row.rawLocation ?? undefined,
      }));

      setItems(mappedItems);
      setStatus(
        mappedItems.length > 0
          ? t("viewer.status.loaded", { count: mappedItems.length })
          : t("viewer.status.empty"),
      );
    } catch {
      setStatus(t("viewer.status.loadFailed"));
    }
  }, [t]);

  useEffect(() => {
    void loadViewerItems();
  }, [loadViewerItems]);

  const onImportArchive = async () => {
    if (isImporting || isExporting) {
      return;
    }

    try {
      setIsImporting(true);
      const picked = await open({
        multiple: false,
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });

      if (!picked || typeof picked !== "string") {
        return;
      }

      const result = await importViewerExportZip(picked);
      await loadViewerItems();
      setStatus(
        t("viewer.import.success", {
          importedCount: result.importedCount,
          skippedCount: result.skippedCount,
        }),
      );
    } catch (error: unknown) {
      const msg = typeof error === "string" ? error : "";
      const isWrongType =
        msg.includes("unsupported archive type") || msg.includes("manifest");
      setStatus(isWrongType ? t("viewer.import.wrongArchiveType") : t("viewer.import.error"));
    } finally {
      setIsImporting(false);
    }
  };

  const onExportArchive = async () => {
    if (isExporting || isImporting) {
      return;
    }

    try {
      setIsExporting(true);
      const picked = await save({
        title: t("viewer.export.saveDialogTitle"),
        defaultPath: "memorysnaper-viewer-export.zip",
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });

      if (!picked || typeof picked !== "string") {
        return;
      }

      const result = await createViewerExportZip(picked);
      setStatus(t("viewer.export.success", { count: result.addedFiles }));
    } catch {
      setStatus(t("viewer.export.error"));
    } finally {
      setIsExporting(false);
    }
  };

  const closeModal = () => {
    const selectedItemId = selectedIndex !== null ? filteredItems[selectedIndex]?.id : undefined;
    setIsModalOpen(false);

    if (!selectedItemId) {
      return;
    }

    window.setTimeout(() => {
      const thumbnailButton = document.getElementById(`viewer-thumb-${selectedItemId}`);
      thumbnailButton?.focus();
    }, 0);
  };

  const openModalAt = (index: number) => {
    if (index < 0 || index >= filteredItems.length) {
      return;
    }

    setSelectedIndex(index);
    setIsModalOpen(true);
  };

  const goPrevious = () => {
    setSelectedIndex((index) => {
      if (index === null || index <= 0) {
        return index;
      }
      return index - 1;
    });
  };

  const goNext = () => {
    setSelectedIndex((index) => {
      if (index === null || index >= filteredItems.length - 1) {
        return index;
      }
      return index + 1;
    });
  };

  useEffect(() => {
    if (!isModalOpen) {
      return;
    }

    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (document.fullscreenElement) {
          void document.exitFullscreen();
          return;
        }
        closeModal();
        return;
      }

      if (event.key === "ArrowLeft") {
        event.preventDefault();
        goPrevious();
        return;
      }

      if (event.key === "ArrowRight") {
        event.preventDefault();
        goNext();
      }
    };

    window.addEventListener("keydown", onKeyDown);

    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [isModalOpen, filteredItems.length, selectedIndex]);

  const gridRows = useMemo<GridTimelineRow[]>(() => {
    const timelineRows: GridTimelineRow[] = [];
    const unknownItems: TimelineThumbnailItem[] = [];
    let pendingRowItems: TimelineThumbnailItem[] = [];
    let pendingStickyHeader: GridStickyHeader | null = null;
    let mediaRowCount = 0;
    let currentYearLabel: string | null = null;
    let currentMonthKey: string | null = null;

    const flushPendingRow = () => {
      if (pendingRowItems.length === 0 || !pendingStickyHeader) {
        return;
      }

      timelineRows.push({
        kind: "media",
        id: `media-row-${mediaRowCount}`,
        items: pendingRowItems,
        stickyHeader: pendingStickyHeader,
      });
      mediaRowCount += 1;
      pendingRowItems = [];
    };

    for (const [mediaIndex, item] of filteredItems.entries()) {
      const timelineItem: TimelineThumbnailItem = {
        id: item.id,
        src: item.thumbnailSrc,
        dateTaken: item.dateTaken,
        mediaKind: item.mediaKind,
        location: item.location,
        rawLocation: item.rawLocation,
        mediaIndex,
      };

      const yearMonth = getViewerYearMonth(item.dateTaken);
      if (!yearMonth) {
        unknownItems.push(timelineItem);
        continue;
      }

      const yearLabel = formatViewerYearLabel(yearMonth);
      if (currentYearLabel !== yearLabel) {
        flushPendingRow();
        currentYearLabel = yearLabel;
        currentMonthKey = null;

        timelineRows.push({
          kind: "year",
          id: `year-${yearLabel}`,
          label: yearLabel,
          stickyHeader: {
            variant: "dated",
            yearLabel,
            monthLabel: null,
          },
        });
      }

      const monthKey = `${yearLabel}-${String(yearMonth.month).padStart(2, "0")}`;
      const monthLabel = formatViewerMonthLabel(yearMonth, resolvedLocale);
      const stickyHeader: GridStickyHeader = {
        variant: "dated",
        yearLabel,
        monthLabel,
      };

      if (currentMonthKey !== monthKey) {
        flushPendingRow();
        currentMonthKey = monthKey;

        timelineRows.push({
          kind: "month",
          id: `month-${monthKey}`,
          label: monthLabel,
          stickyHeader,
        });
      }

      pendingStickyHeader = stickyHeader;
      pendingRowItems.push(timelineItem);

      if (pendingRowItems.length === GRID_COLUMNS) {
        flushPendingRow();
      }
    }

    flushPendingRow();

    if (unknownItems.length > 0) {
      const unknownStickyHeader: GridStickyHeader = {
        variant: "unknown",
        yearLabel: t("viewer.timeline.unknown"),
        monthLabel: null,
      };

      timelineRows.push({
        kind: "unknown",
        id: "unknown-date",
        label: t("viewer.timeline.unknown"),
        stickyHeader: unknownStickyHeader,
      });

      for (let index = 0; index < unknownItems.length; index += GRID_COLUMNS) {
        timelineRows.push({
          kind: "media",
          id: `unknown-media-row-${index}`,
          items: unknownItems.slice(index, index + GRID_COLUMNS),
          stickyHeader: unknownStickyHeader,
        });
      }
    }

    return timelineRows;
  }, [filteredItems, resolvedLocale, t]);

  const currentIndex = selectedIndex ?? -1;
  const modalItems = useMemo(
    () =>
      filteredItems.map((item) => ({
        id: item.id,
        mediaSrc: item.mediaSrc,
        mediaKind: item.mediaKind,
        mediaFormat: item.mediaFormat,
        dateTaken: item.dateTaken,
        location: item.location,
        rawLocation: item.rawLocation,
      })),
    [filteredItems],
  );

  return (
    <Card className="mx-auto flex h-full w-full flex-col">
      <CardHeader>
        <CardTitle>{t("viewer.card.title")}</CardTitle>
        <CardDescription>
          {t("viewer.card.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex min-h-0 flex-1 flex-col space-y-3">
        <ViewerFilterBar
          filters={filterState}
          onChange={setFilterState}
          filterMeta={filterMeta}
          totalCount={items.length}
          filteredCount={filteredItems.length}
        />
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              void onImportArchive();
            }}
            disabled={isImporting || isExporting}
          >
            {isImporting ? t("viewer.import.inProgress") : t("viewer.import.button")}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              void onExportArchive();
            }}
            disabled={isExporting || isImporting}
          >
            {isExporting ? t("viewer.export.inProgress") : t("viewer.export.button")}
          </Button>
        </div>
        <p className="text-sm text-muted-foreground">{status}</p>
        <Grid rows={gridRows} onItemSelect={openModalAt} />
      </CardContent>

      <MediaViewerModal
        open={isModalOpen}
        items={modalItems}
        currentIndex={currentIndex}
        onClose={closeModal}
        onPrevious={goPrevious}
        onNext={goNext}
      />
    </Card>
  );
}

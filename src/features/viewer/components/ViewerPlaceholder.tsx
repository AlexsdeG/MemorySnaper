import { convertFileSrc } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Download, FolderOpen, ImageIcon, CircleHelp } from "lucide-react";
import { toast } from "sonner";

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
  openMediaFolder,
  getViewerItems,
  onProcessProgress,
  type ViewerMediaKind,
} from "@/lib/memories-api";
import { GuideDialog } from "@/components/GuideDialog";
import { getGuideById } from "@/data/guides/index";

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

const VIEWER_PAGE_SIZE = 200;

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
  const [isExporting, setIsExporting] = useState(false);
  const [filterSheetOpen, setFilterSheetOpen] = useState(false);
  const [guideOpen, setGuideOpen] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [isLoadingPage, setIsLoadingPage] = useState(false);
  const isLoadingPageRef = useRef(false);
  const hasMoreRef = useRef(true);
  const nextOffsetRef = useRef(0);
  const cacheVersionRef = useRef(Date.now());
  const reloadTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const viewerGuide = getGuideById("viewer-usage") ?? null;

  const filterMeta = useMemo(() => extractFilterMeta(items), [items]);
  const filteredItems = useMemo(() => applyViewerFilters(items, filterState), [items, filterState]);

  const loadViewerItems = useCallback(async (reset: boolean) => {
    if (isLoadingPageRef.current) {
      return;
    }

    if (!reset && !hasMoreRef.current) {
      return;
    }

    isLoadingPageRef.current = true;
    setIsLoadingPage(true);

    const offset = reset ? 0 : nextOffsetRef.current;

    try {
      const viewerRows = await getViewerItems(offset, VIEWER_PAGE_SIZE);
      const mappedItems = viewerRows.map((row) => ({
        id: String(row.memoryItemId),
        thumbnailSrc: `${convertFileSrc(row.thumbnailPath, "asset")}?v=${cacheVersionRef.current}`,
        mediaSrc: convertFileSrc(row.mediaPath, "asset"),
        mediaKind: row.mediaKind,
        mediaFormat: row.mediaFormat ?? undefined,
        dateTaken: row.dateTaken,
        location: row.location ?? undefined,
        rawLocation: row.rawLocation ?? undefined,
      }));

      const pageHasMore = viewerRows.length === VIEWER_PAGE_SIZE;
      hasMoreRef.current = pageHasMore;
      setHasMore(pageHasMore);
      nextOffsetRef.current = offset + viewerRows.length;

      let nextCount = 0;
      setItems((previous) => {
        if (reset) {
          nextCount = mappedItems.length;
          return mappedItems;
        }

        const knownIds = new Set(previous.map((item) => item.id));
        const appended = mappedItems.filter((item) => !knownIds.has(item.id));
        const merged = [...previous, ...appended];
        nextCount = merged.length;
        return merged;
      });

      setStatus(
        nextCount > 0
          ? t("viewer.status.loaded", { count: nextCount })
          : t("viewer.status.empty"),
      );
    } catch {
      setStatus(t("viewer.status.loadFailed"));
    } finally {
      isLoadingPageRef.current = false;
      setIsLoadingPage(false);
    }
  }, [t]);

  useEffect(() => {
    hasMoreRef.current = true;
    nextOffsetRef.current = 0;
    setHasMore(true);
    setStatus(t("viewer.status.loading"));
    void loadViewerItems(true);
  }, [loadViewerItems]);

  // Listen to process-progress events and debounce-reload the viewer
  // so newly processed thumbnails appear without a manual tab switch.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    const scheduleReload = () => {
      if (reloadTimerRef.current) {
        clearTimeout(reloadTimerRef.current);
      }
      reloadTimerRef.current = setTimeout(() => {
        if (cancelled) return;
        cacheVersionRef.current = Date.now();
        hasMoreRef.current = true;
        nextOffsetRef.current = 0;
        setHasMore(true);
        void loadViewerItems(true);
      }, 2_000);
    };

    onProcessProgress((payload) => {
      if (payload.status === "success" || payload.status === "duplicate") {
        scheduleReload();
      }
    }).then((fn) => {
      if (cancelled) { fn(); return; }
      unlisten = fn;
    });

    return () => {
      cancelled = true;
      if (reloadTimerRef.current) clearTimeout(reloadTimerRef.current);
      unlisten?.();
    };
  }, [loadViewerItems]);

  const onNearEnd = useCallback(() => {
    if (isLoadingPageRef.current || !hasMore || !hasMoreRef.current) {
      return;
    }

    void loadViewerItems(false);
  }, [hasMore, loadViewerItems]);

  const onExportArchive = async () => {
    if (isExporting) {
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
      toast.success(t("viewer.export.success", { count: result.addedFiles }));
    } catch {
      toast.error(t("viewer.export.error"));
    } finally {
      setIsExporting(false);
    }
  };

  const onOpenMediaFolder = async () => {
    try {
      await openMediaFolder();
    } catch {
      toast.error(t("viewer.openFolder.error"));
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
    <div className="mx-auto flex h-full w-full flex-col pb-8 md:pb-10">
      {/* Page Header */}
      <div className="flex flex-col gap-3 pb-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-1">
          <h2 className="text-lg font-semibold tracking-tight">
            {t("viewer.card.title")}
          </h2>
          <p className="text-sm text-muted-foreground">{status}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => setGuideOpen(true)}
          >
            <CircleHelp className="h-3.5 w-3.5" />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => { void onOpenMediaFolder(); }}
            disabled={items.length === 0}
            className="gap-1.5"
          >
            <FolderOpen className="h-3.5 w-3.5" />
            {t("viewer.openFolder.button")}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => { void onExportArchive(); }}
            disabled={isExporting || items.length === 0}
            className="gap-1.5"
          >
            <Download className="h-3.5 w-3.5" />
            {isExporting ? t("viewer.export.inProgress") : t("viewer.export.button")}
          </Button>
        </div>
      </div>

      {/* Filter Bar */}
      <ViewerFilterBar
        filters={filterState}
        onChange={setFilterState}
        filterMeta={filterMeta}
        totalCount={items.length}
        filteredCount={filteredItems.length}
        open={filterSheetOpen}
        onOpenChange={setFilterSheetOpen}
      />

      {/* Grid or Empty State */}
      <div className="mt-3 flex min-h-0 flex-1 flex-col">
        {items.length === 0 && !isLoadingPage && status !== t("viewer.status.loading") ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-4 rounded-lg border-2 border-dashed border-muted-foreground/25 py-16">
            <div className="rounded-full bg-muted/50 p-4">
              <ImageIcon className="h-10 w-10 text-muted-foreground/50" />
            </div>
            <div className="text-center space-y-1">
              <p className="text-sm font-medium text-muted-foreground">
                {t("viewer.empty.title")}
              </p>
              <p className="text-xs text-muted-foreground/70">
                {t("viewer.empty.description")}
              </p>
            </div>
          </div>
        ) : (
          <Grid rows={gridRows} onItemSelect={openModalAt} onNearEnd={onNearEnd} />
        )}
      </div>

      <MediaViewerModal
        open={isModalOpen}
        items={modalItems}
        currentIndex={currentIndex}
        onClose={closeModal}
        onPrevious={goPrevious}
        onNext={goNext}
      />
      <GuideDialog guide={viewerGuide} open={guideOpen} onOpenChange={setGuideOpen} />
    </div>
  );
}

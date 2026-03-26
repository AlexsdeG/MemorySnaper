import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Grid } from "@/features/viewer/components/Grid";
import { MediaViewerModal } from "@/features/viewer/components/MediaViewerModal";
import { useI18n } from "@/lib/i18n";
import { getViewerItems, type ViewerMediaKind } from "@/lib/memories-api";

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

export function ViewerPlaceholder() {
  const { t } = useI18n();
  const [items, setItems] = useState<GridItem[]>([]);
  const [status, setStatus] = useState(t("viewer.status.loading"));
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);

  useEffect(() => {
    const loadViewerItems = async () => {
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

        console.log("[viewer] Loaded viewer rows", {
          count: viewerRows.length,
          sample: viewerRows.slice(0, 3),
        });

        setItems(mappedItems);
        setStatus(
          mappedItems.length > 0
            ? t("viewer.status.loaded", { count: mappedItems.length })
            : t("viewer.status.empty"),
        );
      } catch {
        setStatus(t("viewer.status.loadFailed"));
      }
    };

    void loadViewerItems();
  }, [t]);

  const closeModal = () => {
    const selectedItemId = selectedIndex !== null ? items[selectedIndex]?.id : undefined;
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
    if (index < 0 || index >= items.length) {
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
      if (index === null || index >= items.length - 1) {
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
  }, [isModalOpen, items.length, selectedIndex]);

  const gridItems = useMemo(
    () =>
      items.map((item) => ({
        id: item.id,
        src: item.thumbnailSrc,
        dateTaken: item.dateTaken,
        mediaKind: item.mediaKind,
        location: item.location,
        rawLocation: item.rawLocation,
      })),
    [items],
  );

  const currentIndex = selectedIndex ?? -1;
  const modalItems = useMemo(
    () =>
      items.map((item) => ({
        id: item.id,
        mediaSrc: item.mediaSrc,
        mediaKind: item.mediaKind,
        mediaFormat: item.mediaFormat,
        dateTaken: item.dateTaken,
        location: item.location,
        rawLocation: item.rawLocation,
      })),
    [items],
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
        <p className="text-sm text-muted-foreground">{status}</p>
        <Grid items={gridItems} onItemSelect={openModalAt} />
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

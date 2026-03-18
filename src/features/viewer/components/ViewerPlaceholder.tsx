import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Grid } from "@/features/viewer/components/Grid";
import { useI18n } from "@/lib/i18n";
import { getThumbnails } from "@/lib/memories-api";

type GridItem = {
  id: string;
  src?: string;
};

export function ViewerPlaceholder() {
  const { t } = useI18n();
  const [items, setItems] = useState<GridItem[]>([]);
  const [status, setStatus] = useState(t("viewer.status.loading"));

  useEffect(() => {
    const loadThumbnails = async () => {
      try {
        const thumbnailRows = await getThumbnails(0, 5000);
        const mappedItems = thumbnailRows.map((row) => ({
          id: String(row.memoryItemId),
          src: convertFileSrc(row.thumbnailPath, "asset"),
        }));

        console.log("[viewer] Loaded thumbnail rows", {
          count: thumbnailRows.length,
          sample: thumbnailRows.slice(0, 3),
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

    void loadThumbnails();
  }, [t]);

  return (
    <Card className="w-full max-w-4xl mx-auto">
      <CardHeader>
        <CardTitle>{t("viewer.card.title")}</CardTitle>
        <CardDescription>
          {t("viewer.card.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">{status}</p>
        <Grid items={items} />
      </CardContent>
    </Card>
  );
}

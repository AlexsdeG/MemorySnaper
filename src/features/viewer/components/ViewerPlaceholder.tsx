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
import { getThumbnails } from "@/lib/memories-api";

type GridItem = {
  id: string;
  src?: string;
};

export function ViewerPlaceholder() {
  const [items, setItems] = useState<GridItem[]>([]);
  const [status, setStatus] = useState("Loading thumbnails...");

  useEffect(() => {
    const loadThumbnails = async () => {
      try {
        const thumbnailRows = await getThumbnails(0, 5000);
        const mappedItems = thumbnailRows.map((row) => ({
          id: String(row.memoryItemId),
          src: convertFileSrc(row.thumbnailPath),
        }));

        setItems(mappedItems);
        setStatus(
          mappedItems.length > 0
            ? `Loaded ${mappedItems.length} thumbnails.`
            : "No thumbnails available yet.",
        );
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Could not load thumbnails.";
        setStatus(message);
      }
    };

    void loadThumbnails();
  }, []);

  return (
    <Card className="w-full max-w-4xl mx-auto">
      <CardHeader>
        <CardTitle>Viewer</CardTitle>
        <CardDescription>
          Placeholder for the virtualized media grid and thumbnail previews.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">{status}</p>
        <Grid items={items} />
      </CardContent>
    </Card>
  );
}

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Workflow } from "@/features/downloader/components/Workflow";

export function DownloaderPlaceholder() {
  return (
    <Card className="w-full max-w-4xl mx-auto">
      <CardHeader>
        <CardTitle>Downloader</CardTitle>
        <CardDescription>
          Placeholder workflow UI for importing and downloading memories.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Workflow />
      </CardContent>
    </Card>
  );
}

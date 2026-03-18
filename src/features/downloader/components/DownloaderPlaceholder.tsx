import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Workflow } from "@/features/downloader/components/Workflow";
import { useI18n } from "@/lib/i18n";

export function DownloaderPlaceholder() {
  const { t } = useI18n();

  return (
    <Card className="w-full max-w-4xl mx-auto">
      <CardHeader>
        <CardTitle>{t("downloader.card.title")}</CardTitle>
        <CardDescription>
          {t("downloader.card.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Workflow />
      </CardContent>
    </Card>
  );
}

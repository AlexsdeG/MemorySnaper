import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { SettingsForm } from "@/features/settings/components/SettingsForm";
import { useI18n } from "@/lib/i18n";

export function SettingsPlaceholder() {
  const { t } = useI18n();

  return (
    <Card className="w-full max-w-4xl mx-auto">
      <CardHeader>
        <CardTitle>{t("settings.card.title")}</CardTitle>
        <CardDescription>
          {t("settings.card.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <SettingsForm />
      </CardContent>
    </Card>
  );
}

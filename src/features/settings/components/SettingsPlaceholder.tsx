import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { SettingsForm } from "@/features/settings/components/SettingsForm";

export function SettingsPlaceholder() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Settings</CardTitle>
        <CardDescription>
          Placeholder for rate limit and output path configuration.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <SettingsForm />
      </CardContent>
    </Card>
  );
}

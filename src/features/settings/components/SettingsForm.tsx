import { useEffect, useMemo, useState, type ChangeEvent } from "react";
import { useTheme } from "next-themes";
import { Button } from "@/components/ui/button";
import { readAppSettings, writeAppSettings } from "@/lib/app-settings";
import { parseLanguagePreference, type LanguagePreference } from "@/lib/language";
import { useI18n } from "@/lib/i18n";

const REQUESTS_WARNING_THRESHOLD = 100;
const CONCURRENCY_WARNING_THRESHOLD = 5;

function clampNonNegativeInteger(value: string): number {
  const parsedValue = Number.parseInt(value, 10);
  if (Number.isNaN(parsedValue) || parsedValue < 0) {
    return 0;
  }

  return parsedValue;
}

type ThemeOption = "light" | "dark" | "system";
const languageOptions: LanguagePreference[] = ["system", "en", "de"];

export function SettingsForm() {
  const { theme, setTheme } = useTheme();
  const { languagePreference, resolvedLocale, setLanguagePreference, t } = useI18n();
  const [requestsPerMinute, setRequestsPerMinute] = useState<number>(10);
  const [concurrentDownloads, setConcurrentDownloads] = useState<number>(3);

  useEffect(() => {
    const settings = readAppSettings();
    setRequestsPerMinute(settings.requestsPerMinute);
    setConcurrentDownloads(settings.concurrentDownloads);
  }, []);

  useEffect(() => {
    const currentSettings = readAppSettings();
    writeAppSettings({
      ...currentSettings,
      requestsPerMinute,
      concurrentDownloads,
      languagePreference,
    });
  }, [concurrentDownloads, requestsPerMinute]);

  const showWarning = useMemo(
    () =>
      requestsPerMinute > REQUESTS_WARNING_THRESHOLD ||
      concurrentDownloads > CONCURRENCY_WARNING_THRESHOLD,
    [concurrentDownloads, requestsPerMinute],
  );

  const onRequestsPerMinuteChange = (event: ChangeEvent<HTMLInputElement>) => {
    setRequestsPerMinute(clampNonNegativeInteger(event.target.value));
  };

  const onConcurrentDownloadsChange = (event: ChangeEvent<HTMLInputElement>) => {
    setConcurrentDownloads(clampNonNegativeInteger(event.target.value));
  };

  const onLanguagePreferenceChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setLanguagePreference(parseLanguagePreference(event.target.value));
  };

  return (
    <form className="space-y-4" onSubmit={(event) => event.preventDefault()}>
      <div className="space-y-2">
        <p className="text-sm font-medium">{t("settings.form.appearance")}</p>
        <div className="flex gap-2">
          {(["light", "system", "dark"] as ThemeOption[]).map((option) => (
            <Button
              key={option}
              type="button"
              variant={theme === option ? "default" : "outline"}
              className="flex-1"
              onClick={() => setTheme(option)}
            >
              {option === "light"
                ? t("settings.form.theme.light")
                : option === "dark"
                  ? t("settings.form.theme.dark")
                  : t("settings.form.theme.system")}
            </Button>
          ))}
        </div>
      </div>

      <div className="space-y-2">
        <label htmlFor="language-preference" className="text-sm font-medium">
          {t("settings.form.language")}
        </label>
        <select
          id="language-preference"
          value={languagePreference}
          onChange={onLanguagePreferenceChange}
          className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
        >
          {languageOptions.map((option) => (
            <option key={option} value={option}>
              {option === "system"
                ? t("settings.form.language.system")
                : option === "de"
                  ? t("settings.form.language.de")
                  : t("settings.form.language.en")}
            </option>
          ))}
        </select>
        {languagePreference === "system" ? (
          <p className="text-xs text-muted-foreground">
            {t("settings.form.language.detected", { locale: resolvedLocale.toUpperCase() })}
          </p>
        ) : null}
      </div>

      <div className="space-y-2">
        <label htmlFor="requests-per-minute" className="text-sm font-medium">
          {t("settings.form.requestsPerMinute")}
        </label>
        <input
          id="requests-per-minute"
          type="number"
          min={0}
          step={1}
          value={requestsPerMinute}
          onChange={onRequestsPerMinuteChange}
          className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
        />
      </div>

      <div className="space-y-2">
        <label htmlFor="concurrent-downloads" className="text-sm font-medium">
          {t("settings.form.concurrentDownloads")}
        </label>
        <input
          id="concurrent-downloads"
          type="number"
          min={0}
          step={1}
          value={concurrentDownloads}
          onChange={onConcurrentDownloadsChange}
          className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
        />
      </div>

      {showWarning ? (
        <p className="text-sm text-red-600">
          {t("settings.form.warning")}
        </p>
      ) : null}
    </form>
  );
}

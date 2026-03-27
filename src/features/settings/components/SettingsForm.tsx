import { useEffect, useMemo, useState, type ChangeEvent } from "react";
import { useTheme } from "next-themes";
import { confirm, save } from "@tauri-apps/plugin-dialog";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  clearPersistedAppClientState,
  parseImageOutputFormatPreference,
  parseImageQualityPreference,
  parseThumbnailQualityPreference,
  parseStartupPagePreference,
  parseThemePreference,
  parseVideoProfilePreference,
  readAppSettings,
  writeAppSettings,
  type ImageOutputFormatPreference,
  type ImageQualityPreference,
  type HardwareAccelerationPreference,
  type ThumbnailQualityPreference,
  type StartupPagePreference,
  type ThemePreference,
  type VideoProfilePreference,
} from "@/lib/app-settings";
import { parseLanguagePreference, type LanguagePreference } from "@/lib/language";
import { useI18n } from "@/lib/i18n";
import {
  createSettingsMediaBackupZip,
  createViewerExportZip,
  resetAllAppData,
} from "@/lib/memories-api";

const REQUESTS_WARNING_THRESHOLD = 100;
const CONCURRENCY_WARNING_THRESHOLD = 5;

const startupPageOptions: StartupPagePreference[] = ["system", "downloader", "viewer"];
const thumbnailQualityOptions: ThumbnailQualityPreference[] = ["360p", "480p", "720p", "1080p"];
const videoProfileOptions: VideoProfilePreference[] = [
  "auto",
  "mp4_compatible",
  "linux_webm",
  "mov_fast",
  "mov_high_quality",
];
const imageOutputFormatOptions: ImageOutputFormatPreference[] = ["jpg", "webp", "png"];
const imageQualityOptions: ImageQualityPreference[] = ["full", "balanced", "fast"];
const hardwareAccelerationOptions: HardwareAccelerationPreference[] = ["enabled", "disabled"];
const booleanOptions = [true, false] as const;

function clampNonNegativeInteger(value: string): number {
  const parsedValue = Number.parseInt(value, 10);
  if (Number.isNaN(parsedValue) || parsedValue < 0) {
    return 0;
  }

  return parsedValue;
}

type ThemeOption = "light" | "dark" | "system";
const languageOptions: LanguagePreference[] = ["system", "en", "de"];

function resolveThemePreference(value: string | undefined): ThemePreference {
  if (typeof value !== "string") {
    return "system";
  }

  return parseThemePreference(value);
}

function resolveThumbnailQualityLabel(
  value: ThumbnailQualityPreference,
  t: (key: import("@/lib/i18n-messages").TranslationKey) => string,
): string {
  if (value === "360p") {
    return t("settings.form.thumbnailQuality.360p");
  }

  if (value === "720p") {
    return t("settings.form.thumbnailQuality.720p");
  }

  if (value === "1080p") {
    return t("settings.form.thumbnailQuality.1080p");
  }

  return t("settings.form.thumbnailQuality.480p");
}

function resolveVideoProfileLabel(
  value: VideoProfilePreference,
  t: (key: import("@/lib/i18n-messages").TranslationKey) => string,
): string {
  if (value === "auto") {
    return t("settings.form.videoProfile.auto");
  }

  if (value === "linux_webm") {
    return t("settings.form.videoProfile.linux_webm");
  }

  if (value === "mov_fast") {
    return t("settings.form.videoProfile.mov_fast");
  }

  if (value === "mov_high_quality") {
    return t("settings.form.videoProfile.mov_high_quality");
  }

  return t("settings.form.videoProfile.mp4_compatible");
}

function resolveImageOutputFormatLabel(
  value: ImageOutputFormatPreference,
  t: (key: import("@/lib/i18n-messages").TranslationKey) => string,
): string {
  if (value === "webp") {
    return t("settings.form.imageFormat.webp");
  }

  if (value === "png") {
    return t("settings.form.imageFormat.png");
  }

  return t("settings.form.imageFormat.jpg");
}

function resolveImageQualityLabel(
  value: ImageQualityPreference,
  t: (key: import("@/lib/i18n-messages").TranslationKey) => string,
): string {
  if (value === "balanced") {
    return t("settings.form.imageQuality.balanced");
  }

  if (value === "fast") {
    return t("settings.form.imageQuality.fast");
  }

  return t("settings.form.imageQuality.full");
}

function resolveHardwareAccelerationLabel(
  value: HardwareAccelerationPreference,
  t: (key: import("@/lib/i18n-messages").TranslationKey) => string,
): string {
  if (value === "enabled") {
    return t("settings.form.videoHardwareAcceleration.enabled");
  }

  return t("settings.form.videoHardwareAcceleration.disabled");
}

export function SettingsForm() {
  const { theme, setTheme } = useTheme();
  const { languagePreference, resolvedLocale, setLanguagePreference, t } = useI18n();
  const [requestsPerMinute, setRequestsPerMinute] = useState<number>(10);
  const [concurrentDownloads, setConcurrentDownloads] = useState<number>(3);
  const [startupPagePreference, setStartupPagePreference] = useState<StartupPagePreference>("system");
  const [thumbnailQuality, setThumbnailQuality] = useState<ThumbnailQualityPreference>("480p");
  const [videoProfile, setVideoProfile] = useState<VideoProfilePreference>("auto");
  const [imageOutputFormat, setImageOutputFormat] = useState<ImageOutputFormatPreference>("jpg");
  const [imageQuality, setImageQuality] = useState<ImageQualityPreference>("full");
  const [videoAutoplay, setVideoAutoplay] = useState(true);
  const [videoMutedByDefault, setVideoMutedByDefault] = useState(true);
  const [videoHardwareAcceleration, setVideoHardwareAcceleration] =
    useState<HardwareAccelerationPreference>("disabled");
  const [hasLoadedSettings, setHasLoadedSettings] = useState(false);
  const [isResettingAllData, setIsResettingAllData] = useState(false);
  const [resetErrorMessage, setResetErrorMessage] = useState<string | null>(null);
  const [isCreatingBackup, setIsCreatingBackup] = useState(false);
  const [isCreatingViewerExport, setIsCreatingViewerExport] = useState(false);
  const [backupStatusMessage, setBackupStatusMessage] = useState<string | null>(null);
  const [backupStatusTone, setBackupStatusTone] = useState<"neutral" | "error" | "success">("neutral");

  useEffect(() => {
    const settings = readAppSettings();
    setRequestsPerMinute(settings.requestsPerMinute);
    setConcurrentDownloads(settings.concurrentDownloads);
    setStartupPagePreference(settings.startupPagePreference);
    setThumbnailQuality(settings.thumbnailQuality);
    setVideoProfile(settings.videoProfile);
    setImageOutputFormat(settings.imageOutputFormat);
    setImageQuality(settings.imageQuality);
    setVideoAutoplay(settings.videoAutoplay);
    setVideoMutedByDefault(settings.videoMutedByDefault);
    setVideoHardwareAcceleration(settings.videoHardwareAcceleration);
    setHasLoadedSettings(true);
  }, []);

  useEffect(() => {
    if (!hasLoadedSettings) {
      return;
    }

    writeAppSettings({
      requestsPerMinute,
      concurrentDownloads,
      languagePreference,
      themePreference: resolveThemePreference(theme),
      startupPagePreference,
      thumbnailQuality,
      videoProfile,
      imageOutputFormat,
      imageQuality,
      videoAutoplay,
      videoMutedByDefault,
      videoHardwareAcceleration,
    });
  }, [
    concurrentDownloads,
    hasLoadedSettings,
    languagePreference,
    requestsPerMinute,
    startupPagePreference,
    thumbnailQuality,
    videoProfile,
    imageOutputFormat,
    imageQuality,
    videoAutoplay,
    videoMutedByDefault,
    videoHardwareAcceleration,
    theme,
  ]);

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

  const onStartupPagePreferenceChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setStartupPagePreference(parseStartupPagePreference(event.target.value));
  };

  const onThumbnailQualityChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setThumbnailQuality(parseThumbnailQualityPreference(event.target.value));
  };

  const onVideoProfileChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setVideoProfile(parseVideoProfilePreference(event.target.value));
  };

  const onImageOutputFormatChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setImageOutputFormat(parseImageOutputFormatPreference(event.target.value));
  };

  const onImageQualityChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setImageQuality(parseImageQualityPreference(event.target.value));
  };

  const onVideoAutoplayChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setVideoAutoplay(event.target.value === "true");
  };

  const onVideoMutedByDefaultChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setVideoMutedByDefault(event.target.value === "true");
  };

  const onVideoHardwareAccelerationChange = (event: ChangeEvent<HTMLSelectElement>) => {
    setVideoHardwareAcceleration(event.target.value as HardwareAccelerationPreference);
  };

  const onResetAllData = async () => {
    if (isResettingAllData) {
      return;
    }

    const confirmed = await confirm(t("settings.form.reset.confirm"), {
      title: t("settings.form.reset.confirmTitle"),
      kind: "warning",
    });
    if (!confirmed) {
      return;
    }

    setResetErrorMessage(null);
    setIsResettingAllData(true);

    try {
      await resetAllAppData();
      clearPersistedAppClientState();
      window.location.reload();
    } catch {
      setResetErrorMessage(t("settings.form.reset.error"));
      setIsResettingAllData(false);
    }
  };

  const onCreateBackupZip = async () => {
    if (isCreatingBackup || isCreatingViewerExport) {
      return;
    }

    setBackupStatusMessage(null);
    setBackupStatusTone("neutral");
    setIsCreatingBackup(true);

    try {
      const pickedPath = await save({
        title: t("settings.form.backup.saveDialogTitle"),
        defaultPath: "memorysnaper-media-backup.zip",
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });

      if (!pickedPath || typeof pickedPath !== "string") {
        return;
      }

      const result = await createSettingsMediaBackupZip(pickedPath);
      setBackupStatusTone("success");
      setBackupStatusMessage(
        t("settings.form.backup.success", { count: result.addedFiles }),
      );
    } catch {
      setBackupStatusTone("error");
      setBackupStatusMessage(t("settings.form.backup.error"));
    } finally {
      setIsCreatingBackup(false);
    }
  };

  const onCreateViewerExport = async () => {
    if (isCreatingViewerExport || isCreatingBackup) {
      return;
    }

    setBackupStatusMessage(null);
    setBackupStatusTone("neutral");
    setIsCreatingViewerExport(true);

    try {
      const pickedPath = await save({
        title: t("settings.form.viewerExport.saveDialogTitle"),
        defaultPath: "memorysnaper-viewer-export.zip",
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });

      if (!pickedPath || typeof pickedPath !== "string") {
        return;
      }

      const result = await createViewerExportZip(pickedPath);
      setBackupStatusTone("success");
      setBackupStatusMessage(
        t("settings.form.viewerExport.success", { count: result.addedFiles }),
      );
    } catch {
      setBackupStatusTone("error");
      setBackupStatusMessage(t("settings.form.viewerExport.error"));
    } finally {
      setIsCreatingViewerExport(false);
    }
  };

  return (
    <form className="space-y-4" onSubmit={(event) => event.preventDefault()}>
      <Card>
        <CardHeader>
          <CardTitle>{t("settings.form.section.interface")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
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
            <label htmlFor="startup-page-preference" className="text-sm font-medium">
              {t("settings.form.startupPage")}
            </label>
            <select
              id="startup-page-preference"
              value={startupPagePreference}
              onChange={onStartupPagePreferenceChange}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              {startupPageOptions.map((option) => (
                <option key={option} value={option}>
                  {option === "system"
                    ? t("settings.form.startupPage.system")
                    : option === "downloader"
                      ? t("settings.form.startupPage.downloader")
                      : t("settings.form.startupPage.viewer")}
                </option>
              ))}
            </select>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.form.section.processing")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
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

          {showWarning ? <p className="text-sm text-red-600">{t("settings.form.warning")}</p> : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.form.section.mediaOutput")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <label htmlFor="thumbnail-quality" className="text-sm font-medium">
              {t("settings.form.thumbnailQuality")}
            </label>
            <select
              id="thumbnail-quality"
              value={thumbnailQuality}
              onChange={onThumbnailQualityChange}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              {thumbnailQualityOptions.map((option) => (
                <option key={option} value={option}>
                  {resolveThumbnailQualityLabel(option, t)}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label htmlFor="video-profile" className="text-sm font-medium">
              {t("settings.form.videoProfile")}
            </label>
            <select
              id="video-profile"
              value={videoProfile}
              onChange={onVideoProfileChange}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              {videoProfileOptions.map((option) => (
                <option key={option} value={option}>
                  {resolveVideoProfileLabel(option, t)}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label htmlFor="image-output-format" className="text-sm font-medium">
              {t("settings.form.imageFormat")}
            </label>
            <select
              id="image-output-format"
              value={imageOutputFormat}
              onChange={onImageOutputFormatChange}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              {imageOutputFormatOptions.map((option) => (
                <option key={option} value={option}>
                  {resolveImageOutputFormatLabel(option, t)}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label htmlFor="image-quality" className="text-sm font-medium">
              {t("settings.form.imageQuality")}
            </label>
            <select
              id="image-quality"
              value={imageQuality}
              onChange={onImageQualityChange}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              {imageQualityOptions.map((option) => (
                <option key={option} value={option}>
                  {resolveImageQualityLabel(option, t)}
                </option>
              ))}
            </select>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.form.section.viewerPlayback")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <label htmlFor="video-autoplay" className="text-sm font-medium">
              {t("settings.form.videoAutoplay")}
            </label>
            <select
              id="video-autoplay"
              value={String(videoAutoplay)}
              onChange={onVideoAutoplayChange}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              {booleanOptions.map((value) => (
                <option key={String(value)} value={String(value)}>
                  {value ? t("settings.form.boolean.true") : t("settings.form.boolean.false")}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label htmlFor="video-muted-default" className="text-sm font-medium">
              {t("settings.form.videoMutedByDefault")}
            </label>
            <select
              id="video-muted-default"
              value={String(videoMutedByDefault)}
              onChange={onVideoMutedByDefaultChange}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              {booleanOptions.map((value) => (
                <option key={String(value)} value={String(value)}>
                  {value ? t("settings.form.boolean.true") : t("settings.form.boolean.false")}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label htmlFor="video-hardware-accel" className="text-sm font-medium">
              {t("settings.form.videoHardwareAcceleration")}
            </label>
            <select
              id="video-hardware-accel"
              value={videoHardwareAcceleration}
              onChange={onVideoHardwareAccelerationChange}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              {hardwareAccelerationOptions.map((option) => (
                <option key={option} value={option}>
                  {resolveHardwareAccelerationLabel(option, t)}
                </option>
              ))}
            </select>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.form.section.backupExport")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <Button
            type="button"
            variant="outline"
            className="w-full"
            disabled={isCreatingBackup || isCreatingViewerExport}
            onClick={() => {
              void onCreateBackupZip();
            }}
          >
            {isCreatingBackup ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                {t("settings.form.backup.inProgress")}
              </>
            ) : (
              t("settings.form.backup.button")
            )}
          </Button>
          <p className="text-xs text-muted-foreground">{t("settings.form.backup.description")}</p>

          <Button
            type="button"
            variant="outline"
            className="w-full"
            disabled={isCreatingViewerExport || isCreatingBackup}
            onClick={() => {
              void onCreateViewerExport();
            }}
          >
            {isCreatingViewerExport ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                {t("settings.form.viewerExport.inProgress")}
              </>
            ) : (
              t("settings.form.viewerExport.button")
            )}
          </Button>
          <p className="text-xs text-muted-foreground">{t("settings.form.viewerExport.description")}</p>

          {backupStatusMessage ? (
            <p
              className={`text-sm ${
                backupStatusTone === "error"
                  ? "text-red-600"
                  : backupStatusTone === "success"
                    ? "text-green-600"
                    : "text-muted-foreground"
              }`}
            >
              {backupStatusMessage}
            </p>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.form.section.dataReset")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          <Button
            type="button"
            variant="destructive"
            className="w-full"
            disabled={isResettingAllData}
            onClick={() => {
              void onResetAllData();
            }}
          >
            {isResettingAllData
              ? t("settings.form.reset.inProgress")
              : t("settings.form.reset.button")}
          </Button>
          <p className="text-xs text-muted-foreground">{t("settings.form.reset.description")}</p>
          {resetErrorMessage ? <p className="text-sm text-red-600">{resetErrorMessage}</p> : null}
        </CardContent>
      </Card>
    </form>
  );
}

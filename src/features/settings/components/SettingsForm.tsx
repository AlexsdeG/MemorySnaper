import { useEffect, useMemo, useState } from "react";
import { useTheme } from "next-themes";
import { save, open } from "@tauri-apps/plugin-dialog";
import { Loader2, Monitor, Moon, Sun, FolderOpen, RotateCcw, BookOpen } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  applyAccentColor,
  clearPersistedAppClientState,
  parseImageOutputFormatPreference,
  parseImageQualityPreference,
  parseThumbnailQualityPreference,
  parseStartupPagePreference,
  parseThemePreference,
  parseVideoProfilePreference,
  readAppSettings,
  writeAppSettings,
  type AccentColor,
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
import type { TranslationKey } from "@/lib/i18n-messages";
import {
  createSettingsMediaBackupZip,
  createViewerExportZip,
  getDefaultExportPath,
  getExportPath,
  resetAllAppData,
  setExportPath,
} from "@/lib/memories-api";
import { cn } from "@/lib/utils";
import { HelpTooltip } from "@/components/HelpTooltip";
import { GuideDialog } from "@/components/GuideDialog";
import { getGuideById } from "@/data/guides/index";

const REQUESTS_WARNING_THRESHOLD = 100;
const CONCURRENCY_WARNING_THRESHOLD = 5;

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

const ACCENT_COLORS: { value: AccentColor; swatch: string }[] = [
  { value: "yellow", swatch: "bg-yellow-400" },
  { value: "blue", swatch: "bg-blue-500" },
  { value: "purple", swatch: "bg-purple-500" },
  { value: "green", swatch: "bg-green-500" },
  { value: "rose", swatch: "bg-rose-500" },
];

const accentLabelKeys: Record<AccentColor, TranslationKey> = {
  yellow: "settings.form.accentColor.yellow",
  blue: "settings.form.accentColor.blue",
  purple: "settings.form.accentColor.purple",
  green: "settings.form.accentColor.green",
  rose: "settings.form.accentColor.rose",
};

function clampNonNegativeInteger(value: string): number {
  const parsedValue = Number.parseInt(value, 10);
  if (Number.isNaN(parsedValue) || parsedValue < 0) {
    return 0;
  }

  return parsedValue;
}

type ThemeOption = "light" | "dark" | "system";
const languageOptions: LanguagePreference[] = [
  "system",
  "en",
  "de",
  "fr",
  "es",
  "it",
  "pl",
  "nl",
  "pt",
];

function resolveThemePreference(value: string | undefined): ThemePreference {
  if (typeof value !== "string") {
    return "system";
  }

  return parseThemePreference(value);
}

const thumbnailQualityLabelKeys: Record<ThumbnailQualityPreference, TranslationKey> = {
  "360p": "settings.form.thumbnailQuality.360p",
  "480p": "settings.form.thumbnailQuality.480p",
  "720p": "settings.form.thumbnailQuality.720p",
  "1080p": "settings.form.thumbnailQuality.1080p",
};

const videoProfileLabelKeys: Record<VideoProfilePreference, TranslationKey> = {
  auto: "settings.form.videoProfile.auto",
  mp4_compatible: "settings.form.videoProfile.mp4_compatible",
  linux_webm: "settings.form.videoProfile.linux_webm",
  mov_fast: "settings.form.videoProfile.mov_fast",
  mov_high_quality: "settings.form.videoProfile.mov_high_quality",
};

const imageFormatLabelKeys: Record<ImageOutputFormatPreference, TranslationKey> = {
  jpg: "settings.form.imageFormat.jpg",
  webp: "settings.form.imageFormat.webp",
  png: "settings.form.imageFormat.png",
};

const imageQualityLabelKeys: Record<ImageQualityPreference, TranslationKey> = {
  full: "settings.form.imageQuality.full",
  balanced: "settings.form.imageQuality.balanced",
  fast: "settings.form.imageQuality.fast",
};

const hwAccelLabelKeys: Record<HardwareAccelerationPreference, TranslationKey> = {
  enabled: "settings.form.videoHardwareAcceleration.enabled",
  disabled: "settings.form.videoHardwareAcceleration.disabled",
};

const languageLabelKeys: Record<LanguagePreference, TranslationKey> = {
  system: "settings.form.language.system",
  en: "settings.form.language.en",
  de: "settings.form.language.de",
  fr: "settings.form.language.fr",
  es: "settings.form.language.es",
  it: "settings.form.language.it",
  pl: "settings.form.language.pl",
  nl: "settings.form.language.nl",
  pt: "settings.form.language.pt",
};

const startupPageLabelKeys: Record<StartupPagePreference, TranslationKey> = {
  system: "settings.form.startupPage.system",
  downloader: "settings.form.startupPage.downloader",
  viewer: "settings.form.startupPage.viewer",
};

export function SettingsForm() {
  const { theme, setTheme } = useTheme();
  const { languagePreference, resolvedLocale, setLanguagePreference, t } = useI18n();
  const [activeTab, setActiveTab] = useState("interface");
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
  const [accentColor, setAccentColor] = useState<AccentColor>("yellow");
  const [hasLoadedSettings, setHasLoadedSettings] = useState(false);
  const [isResettingAllData, setIsResettingAllData] = useState(false);
  const [isCreatingBackup, setIsCreatingBackup] = useState(false);
  const [isCreatingViewerExport, setIsCreatingViewerExport] = useState(false);
  const [resetDialogOpen, setResetDialogOpen] = useState(false);
  const [currentExportPath, setCurrentExportPath] = useState<string>("");
  const [defaultExportPath, setDefaultExportPath] = useState<string>("");
  const [isChangingExportPath, setIsChangingExportPath] = useState(false);
  const [setupGuideOpen, setSetupGuideOpen] = useState(false);
  const setupGuide = getGuideById("first-time-setup") ?? null;

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
    setAccentColor(settings.accentColor);
    setHasLoadedSettings(true);

    void Promise.all([getExportPath(), getDefaultExportPath()]).then(([current, defaultP]) => {
      setCurrentExportPath(current);
      setDefaultExportPath(defaultP);
    });
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
      accentColor,
      exportPath: currentExportPath !== defaultExportPath ? currentExportPath : null,
    });
  }, [
    accentColor,
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

  const onAccentColorChange = (color: AccentColor) => {
    setAccentColor(color);
    applyAccentColor(color);
  };

  const isCustomExportPath = currentExportPath !== "" && currentExportPath !== defaultExportPath;

  const onChangeExportPath = async () => {
    if (isChangingExportPath) {
      return;
    }

    setIsChangingExportPath(true);
    try {
      const picked = await open({
        title: t("settings.form.exportPath.dialogTitle"),
        directory: true,
        multiple: false,
      });
      if (!picked || typeof picked !== "string") {
        return;
      }

      const resolved = await setExportPath(picked);
      setCurrentExportPath(resolved);

      const settings = readAppSettings();
      writeAppSettings({ ...settings, exportPath: picked });

      toast.success(t("settings.form.exportPath.success"));
    } catch {
      toast.error(t("settings.form.exportPath.error"));
    } finally {
      setIsChangingExportPath(false);
    }
  };

  const onResetExportPath = async () => {
    if (isChangingExportPath) {
      return;
    }

    setIsChangingExportPath(true);
    try {
      const resolved = await setExportPath(null);
      setCurrentExportPath(resolved);

      const settings = readAppSettings();
      writeAppSettings({ ...settings, exportPath: null });

      toast.success(t("settings.form.exportPath.resetSuccess"));
    } catch {
      toast.error(t("settings.form.exportPath.error"));
    } finally {
      setIsChangingExportPath(false);
    }
  };

  const onResetAllData = async () => {
    if (isResettingAllData) {
      return;
    }

    setIsResettingAllData(true);

    try {
      await resetAllAppData();
      clearPersistedAppClientState();
      window.location.reload();
    } catch {
      toast.error(t("settings.form.reset.error"));
      setIsResettingAllData(false);
    }
  };

  const onCreateBackupZip = async () => {
    if (isCreatingBackup || isCreatingViewerExport) {
      return;
    }

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
      toast.success(t("settings.form.backup.success", { count: result.addedFiles }));
    } catch {
      toast.error(t("settings.form.backup.error"));
    } finally {
      setIsCreatingBackup(false);
    }
  };

  const onCreateViewerExport = async () => {
    if (isCreatingViewerExport || isCreatingBackup) {
      return;
    }

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
      toast.success(t("settings.form.viewerExport.success", { count: result.addedFiles }));
    } catch {
      toast.error(t("settings.form.viewerExport.error"));
    } finally {
      setIsCreatingViewerExport(false);
    }
  };

  return (
    <Tabs defaultValue="interface" value={activeTab} onValueChange={setActiveTab} className="flex flex-col bg-background rounded-lg">
      <TabsList className="w-full justify-start overflow-x-auto overflow-y-hidden">
        <TabsTrigger value="interface">{t("settings.form.section.interface")}</TabsTrigger>
        <TabsTrigger value="processing">{t("settings.form.section.processing")}</TabsTrigger>
        <TabsTrigger value="media">{t("settings.form.section.mediaOutput")}</TabsTrigger>
        <TabsTrigger value="playback">{t("settings.form.section.viewerPlayback")}</TabsTrigger>
        <TabsTrigger value="data">{t("settings.form.section.data")}</TabsTrigger>
      </TabsList>

      {/* ── Interface ── */}
      <TabsContent value="interface" className="space-y-6 pt-4 pb-8">
        {/* Theme */}
        <div className="space-y-2">
          <Label className="text-sm font-medium">{t("settings.form.appearance")}</Label>
          <ToggleGroup
            type="single"
            size="sm"
            variant="outline"
            value={theme ?? "system"}
            onValueChange={(value) => { if (value) setTheme(value as ThemeOption); }}
          >
            <ToggleGroupItem value="light" className="gap-1.5">
              <Sun className="size-4" />
              {t("settings.form.theme.light")}
            </ToggleGroupItem>
            <ToggleGroupItem value="system" className="gap-1.5">
              <Monitor className="size-4" />
              {t("settings.form.theme.system")}
            </ToggleGroupItem>
            <ToggleGroupItem value="dark" className="gap-1.5">
              <Moon className="size-4" />
              {t("settings.form.theme.dark")}
            </ToggleGroupItem>
          </ToggleGroup>
        </div>

        {/* Language */}
        <div className="space-y-2">
          <Label>{t("settings.form.language")}</Label>
          <Select
            value={languagePreference}
            onValueChange={(value) => { setLanguagePreference(parseLanguagePreference(value)); }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {languageOptions.map((option) => (
                <SelectItem key={option} value={option}>
                  {t(languageLabelKeys[option])}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {languagePreference === "system" ? (
            <p className="text-xs text-muted-foreground">
              {t("settings.form.language.detected", { locale: resolvedLocale.toUpperCase() })}
            </p>
          ) : null}
        </div>

        {/* Startup Page */}
        <div className="space-y-2">
          <Label>{t("settings.form.startupPage")}</Label>
          <Select
            value={startupPagePreference}
            onValueChange={(value) => { setStartupPagePreference(parseStartupPagePreference(value)); }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(["system", "downloader", "viewer"] as StartupPagePreference[]).map((option) => (
                <SelectItem key={option} value={option}>
                  {t(startupPageLabelKeys[option])}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <Separator />

        {/* Accent Color */}
        {activeTab === "interface" && (
          <div className="space-y-2">
            <Label>{t("settings.form.accentColor")}</Label>
            <div className="flex items-center gap-3 mr-12 ml-2">
              {ACCENT_COLORS.map(({ value, swatch }) => (
                <button
                  key={value}
                  type="button"
                  aria-label={t(accentLabelKeys[value])}
                  className={cn(
                    "size-8 rounded-full transition-transform focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
                    swatch,
                    accentColor === value
                      ? "ring-2 ring-ring ring-offset-2 ring-offset-background scale-110"
                      : "hover:scale-105",
                  )}
                  onClick={() => { onAccentColorChange(value); }}
                />
              ))}
            </div>
          </div>
        )}
      </TabsContent>

      {/* ── Processing ── */}
      <TabsContent value="processing" className="space-y-6 pt-4 pb-8">
        <div className="space-y-2">
          <div className="flex items-center gap-1.5">
            <Label htmlFor="requests-per-minute">{t("settings.form.requestsPerMinute")}</Label>
            <HelpTooltip helpKey="help.settings.requestsPerMinute" />
          </div>
          <Input
            id="requests-per-minute"
            type="number"
            min={0}
            step={1}
            value={requestsPerMinute}
            onChange={(event) => { setRequestsPerMinute(clampNonNegativeInteger(event.target.value)); }}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="concurrent-downloads">{t("settings.form.concurrentDownloads")}</Label>
          <Input
            id="concurrent-downloads"
            type="number"
            min={0}
            step={1}
            value={concurrentDownloads}
            onChange={(event) => { setConcurrentDownloads(clampNonNegativeInteger(event.target.value)); }}
          />
        </div>

        {showWarning ? <p className="text-sm text-red-600">{t("settings.form.warning")}</p> : null}
      </TabsContent>

      {/* ── Media Output ── */}
      <TabsContent value="media" className="space-y-6 pt-4 pb-8">
        <div className="space-y-2">
          <div className="flex items-center gap-1.5">
            <Label>{t("settings.form.thumbnailQuality")}</Label>
            <HelpTooltip helpKey="help.settings.thumbnailQuality" />
          </div>
          <Select
            value={thumbnailQuality}
            onValueChange={(value) => { setThumbnailQuality(parseThumbnailQualityPreference(value)); }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {thumbnailQualityOptions.map((option) => (
                <SelectItem key={option} value={option}>
                  {t(thumbnailQualityLabelKeys[option])}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1.5">
            <Label>{t("settings.form.videoProfile")}</Label>
            <HelpTooltip helpKey="help.settings.videoProfile" />
          </div>
          <Select
            value={videoProfile}
            onValueChange={(value) => { setVideoProfile(parseVideoProfilePreference(value)); }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {videoProfileOptions.map((option) => (
                <SelectItem key={option} value={option}>
                  {t(videoProfileLabelKeys[option])}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1.5">
            <Label>{t("settings.form.imageFormat")}</Label>
            <HelpTooltip helpKey="help.settings.imageFormat" />
          </div>
          <Select
            value={imageOutputFormat}
            onValueChange={(value) => { setImageOutputFormat(parseImageOutputFormatPreference(value)); }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {imageOutputFormatOptions.map((option) => (
                <SelectItem key={option} value={option}>
                  {t(imageFormatLabelKeys[option])}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-2">
          <Label>{t("settings.form.imageQuality")}</Label>
          <Select
            value={imageQuality}
            onValueChange={(value) => { setImageQuality(parseImageQualityPreference(value)); }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {imageQualityOptions.map((option) => (
                <SelectItem key={option} value={option}>
                  {t(imageQualityLabelKeys[option])}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </TabsContent>

      {/* ── Playback ── */}
      <TabsContent value="playback" className="space-y-6 pt-4 pb-8">
        <div className="flex items-center justify-between">
          <Label htmlFor="video-autoplay">{t("settings.form.videoAutoplay")}</Label>
          <Switch
            id="video-autoplay"
            checked={videoAutoplay}
            onCheckedChange={setVideoAutoplay}
          />
        </div>

        <div className="flex items-center justify-between">
          <Label htmlFor="video-muted-default">{t("settings.form.videoMutedByDefault")}</Label>
          <Switch
            id="video-muted-default"
            checked={videoMutedByDefault}
            onCheckedChange={setVideoMutedByDefault}
          />
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1.5">
            <Label>{t("settings.form.videoHardwareAcceleration")}</Label>
            <HelpTooltip helpKey="help.settings.hwAcceleration" />
          </div>
          <Select
            value={videoHardwareAcceleration}
            onValueChange={(value) => { setVideoHardwareAcceleration(value as HardwareAccelerationPreference); }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(["enabled", "disabled"] as HardwareAccelerationPreference[]).map((option) => (
                <SelectItem key={option} value={option}>
                  {t(hwAccelLabelKeys[option])}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </TabsContent>

      {/* ── Data ── */}
      <TabsContent value="data" className="space-y-6 pt-4 pb-8">
        {/* Storage Location */}
        <div className="space-y-3">
          <Label className="text-sm font-medium">{t("settings.form.section.storageLocation")}</Label>
          <p className="text-xs text-muted-foreground">{t("settings.form.exportPath.description")}</p>

          <div className="flex items-center gap-2 rounded-md bg-muted/40 px-3 py-2">
            <FolderOpen className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={currentExportPath}>
              {isCustomExportPath ? currentExportPath : t("settings.form.exportPath.default")}
            </span>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={isChangingExportPath}
              onClick={() => { void onChangeExportPath(); }}
              className="gap-1.5"
            >
              {isChangingExportPath ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <FolderOpen className="h-3.5 w-3.5" />
              )}
              {t("settings.form.exportPath.change")}
            </Button>
            {isCustomExportPath && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={isChangingExportPath}
                onClick={() => { void onResetExportPath(); }}
                className="gap-1.5 text-muted-foreground"
              >
                <RotateCcw className="h-3.5 w-3.5" />
                {t("settings.form.exportPath.reset")}
              </Button>
            )}
          </div>
        </div>

        <Separator />

        {/* Backup & Export */}
        <div className="space-y-3">
          <Label className="text-sm font-medium">{t("settings.form.section.backupExport")}</Label>

          <Button
            type="button"
            variant="outline"
            className="w-full"
            disabled={isCreatingBackup || isCreatingViewerExport}
            onClick={() => { void onCreateBackupZip(); }}
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
            onClick={() => { void onCreateViewerExport(); }}
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


        </div>

        <Separator />

        {/* Setup Guide */}
        <div className="space-y-3">
          <Button
            type="button"
            variant="outline"
            className="w-full gap-1.5"
            onClick={() => setSetupGuideOpen(true)}
          >
            <BookOpen className="h-4 w-4" />
            {t("settings.data.showSetupGuide")}
          </Button>
          <GuideDialog guide={setupGuide} open={setupGuideOpen} onOpenChange={setSetupGuideOpen} />
        </div>

        <Separator />

        {/* Data Reset */}
        <div className="space-y-3">
          <Label className="text-sm font-medium">{t("settings.form.section.dataReset")}</Label>
          <p className="text-xs text-muted-foreground">{t("settings.form.reset.description")}</p>

          <Dialog open={resetDialogOpen} onOpenChange={setResetDialogOpen}>
            <DialogTrigger asChild>
              <Button
                type="button"
                variant="destructive"
                className="w-full"
                disabled={isResettingAllData}
              >
                {isResettingAllData
                  ? t("settings.form.reset.inProgress")
                  : t("settings.form.reset.button")}
              </Button>
            </DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>{t("settings.form.reset.confirmTitle")}</DialogTitle>
                <DialogDescription>
                  {t("settings.form.reset.confirm")}
                </DialogDescription>
              </DialogHeader>
              <DialogFooter className="gap-2 sm:gap-0">
                <DialogClose asChild>
                  <Button type="button" variant="outline">
                    {t("settings.form.reset.cancel")}
                  </Button>
                </DialogClose>
                <Button
                  type="button"
                  variant="destructive"
                  disabled={isResettingAllData}
                  onClick={() => {
                    setResetDialogOpen(false);
                    void onResetAllData();
                  }}
                >
                  {t("settings.form.reset.button")}
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>


        </div>
      </TabsContent>
    </Tabs>
  );
}

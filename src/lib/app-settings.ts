import { parseLanguagePreference, type LanguagePreference } from "@/lib/language";

export const SETTINGS_STORAGE_KEY = "memorysnaper.rate-limit-settings";
export const THEME_STORAGE_KEY = "memorysnaper-theme";
export const DOWNLOADER_SESSION_STORAGE_KEY = "memorysnaper.downloader-session.v1";

export type ThemePreference = "light" | "dark" | "system";
export type StartupPagePreference = "system" | "downloader" | "viewer";
export type ThumbnailQualityPreference = "360p" | "480p" | "720p" | "1080p";
export type HardwareAccelerationPreference = "enabled" | "disabled";
export type VideoProfilePreference =
  | "auto"
  | "mp4_compatible"
  | "linux_webm"
  | "mov_fast"
  | "mov_high_quality";
export type ImageOutputFormatPreference = "jpg" | "webp" | "png";
export type ImageQualityPreference = "full" | "balanced" | "fast";
export type EncodingHwAccelPreference = "auto" | "nvenc" | "qsv" | "vaapi" | "disabled";
export type OverlayStrategyPreference = "upscale" | "downscale_sharpen";
export type AccentColor = "yellow" | "blue" | "purple" | "green" | "rose";

export type AppSettings = {
  requestsPerMinute: number;
  concurrentDownloads: number;
  languagePreference: LanguagePreference;
  themePreference: ThemePreference;
  startupPagePreference: StartupPagePreference;
  thumbnailQuality: ThumbnailQualityPreference;
  videoProfile: VideoProfilePreference;
  imageOutputFormat: ImageOutputFormatPreference;
  imageQuality: ImageQualityPreference;
  encodingHwAccel: EncodingHwAccelPreference;
  overlayStrategy: OverlayStrategyPreference;
  videoAutoplay: boolean;
  videoMutedByDefault: boolean;
  videoHardwareAcceleration: HardwareAccelerationPreference;
  accentColor: AccentColor;
  exportPath: string | null;
};

const DEFAULT_SETTINGS: AppSettings = {
  requestsPerMinute: 10,
  concurrentDownloads: 3,
  languagePreference: "system",
  themePreference: "system",
  startupPagePreference: "system",
  thumbnailQuality: "480p",
  videoProfile: "auto",
  imageOutputFormat: "jpg",
  imageQuality: "full",
  encodingHwAccel: "auto",
  overlayStrategy: "upscale",
  videoAutoplay: true,
  videoMutedByDefault: true,
  videoHardwareAcceleration: "disabled",
  accentColor: "yellow",
  exportPath: null,
};

function parseBooleanSetting(value: unknown, fallback: boolean): boolean {
  if (typeof value === "boolean") {
    return value;
  }

  return fallback;
}

export function parseStartupPagePreference(value: string | null): StartupPagePreference {
  if (value === "system" || value === "downloader" || value === "viewer") {
    return value;
  }

  return "system";
}

export function parseThemePreference(value: string | null): ThemePreference {
  if (value === "light" || value === "dark" || value === "system") {
    return value;
  }

  return "system";
}

export function parseThumbnailQualityPreference(
  value: string | null,
): ThumbnailQualityPreference {
  if (value === "360p" || value === "480p" || value === "720p" || value === "1080p") {
    return value;
  }

  return "480p";
}

export function parseVideoProfilePreference(value: string | null): VideoProfilePreference {
  if (
    value === "auto" ||
    value === "mp4_compatible" ||
    value === "linux_webm" ||
    value === "mov_fast" ||
    value === "mov_high_quality"
  ) {
    return value;
  }

  return "auto";
}

export function parseImageOutputFormatPreference(value: string | null): ImageOutputFormatPreference {
  if (value === "jpg" || value === "webp" || value === "png") {
    return value;
  }

  return "jpg";
}

export function parseImageQualityPreference(value: string | null): ImageQualityPreference {
  if (value === "full" || value === "balanced" || value === "fast") {
    return value;
  }

  return "full";
}

export function parseEncodingHwAccelPreference(
  value: string | null,
): EncodingHwAccelPreference {
  if (
    value === "auto" ||
    value === "nvenc" ||
    value === "qsv" ||
    value === "vaapi" ||
    value === "disabled"
  ) {
    return value;
  }

  return "auto";
}

export function parseOverlayStrategyPreference(
  value: string | null,
): OverlayStrategyPreference {
  if (value === "upscale" || value === "downscale_sharpen") {
    return value;
  }

  return "upscale";
}

export function parseHardwareAccelerationPreference(
  value: string | null,
): HardwareAccelerationPreference {
  if (value === "enabled" || value === "disabled") {
    return value;
  }

  return "disabled";
}

export function parseAccentColor(value: string | null): AccentColor {
  if (
    value === "yellow" ||
    value === "blue" ||
    value === "purple" ||
    value === "green" ||
    value === "rose"
  ) {
    return value;
  }

  return "yellow";
}

function normalizeNonNegativeInteger(value: unknown, fallback: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }

  return Math.max(0, Math.floor(value));
}

function parseSettings(rawValue: string): AppSettings | null {
  try {
    const parsedValue: unknown = JSON.parse(rawValue);
    if (!parsedValue || typeof parsedValue !== "object") {
      return null;
    }

    const requestsPerMinute = normalizeNonNegativeInteger(
      Reflect.get(parsedValue, "requestsPerMinute"),
      DEFAULT_SETTINGS.requestsPerMinute,
    );
    const concurrentDownloads = normalizeNonNegativeInteger(
      Reflect.get(parsedValue, "concurrentDownloads"),
      DEFAULT_SETTINGS.concurrentDownloads,
    );
    const languagePreference = parseLanguagePreference(
      typeof Reflect.get(parsedValue, "languagePreference") === "string"
        ? (Reflect.get(parsedValue, "languagePreference") as string)
        : null,
    );
    const themePreference = parseThemePreference(
      typeof Reflect.get(parsedValue, "themePreference") === "string"
        ? (Reflect.get(parsedValue, "themePreference") as string)
        : null,
    );
    const startupPagePreference = parseStartupPagePreference(
      typeof Reflect.get(parsedValue, "startupPagePreference") === "string"
        ? (Reflect.get(parsedValue, "startupPagePreference") as string)
        : null,
    );
    const thumbnailQuality = parseThumbnailQualityPreference(
      typeof Reflect.get(parsedValue, "thumbnailQuality") === "string"
        ? (Reflect.get(parsedValue, "thumbnailQuality") as string)
        : null,
    );
    const videoProfile = parseVideoProfilePreference(
      typeof Reflect.get(parsedValue, "videoProfile") === "string"
        ? (Reflect.get(parsedValue, "videoProfile") as string)
        : null,
    );
    const imageOutputFormat = parseImageOutputFormatPreference(
      typeof Reflect.get(parsedValue, "imageOutputFormat") === "string"
        ? (Reflect.get(parsedValue, "imageOutputFormat") as string)
        : null,
    );
    const imageQuality = parseImageQualityPreference(
      typeof Reflect.get(parsedValue, "imageQuality") === "string"
        ? (Reflect.get(parsedValue, "imageQuality") as string)
        : null,
    );
    const encodingHwAccel = parseEncodingHwAccelPreference(
      typeof Reflect.get(parsedValue, "encodingHwAccel") === "string"
        ? (Reflect.get(parsedValue, "encodingHwAccel") as string)
        : null,
    );
    const overlayStrategy = parseOverlayStrategyPreference(
      typeof Reflect.get(parsedValue, "overlayStrategy") === "string"
        ? (Reflect.get(parsedValue, "overlayStrategy") as string)
        : null,
    );
    const videoAutoplay = parseBooleanSetting(
      Reflect.get(parsedValue, "videoAutoplay"),
      DEFAULT_SETTINGS.videoAutoplay,
    );
    const videoMutedByDefault = parseBooleanSetting(
      Reflect.get(parsedValue, "videoMutedByDefault"),
      DEFAULT_SETTINGS.videoMutedByDefault,
    );
    const videoHardwareAcceleration = parseHardwareAccelerationPreference(
      typeof Reflect.get(parsedValue, "videoHardwareAcceleration") === "string"
        ? (Reflect.get(parsedValue, "videoHardwareAcceleration") as string)
        : null,
    );
    const accentColor = parseAccentColor(
      typeof Reflect.get(parsedValue, "accentColor") === "string"
        ? (Reflect.get(parsedValue, "accentColor") as string)
        : null,
    );
    const rawExportPath = Reflect.get(parsedValue, "exportPath");
    const exportPath = typeof rawExportPath === "string" && rawExportPath.length > 0
      ? rawExportPath
      : null;

    return {
      requestsPerMinute,
      concurrentDownloads,
      languagePreference,
      themePreference,
      startupPagePreference,
      thumbnailQuality,
      videoProfile,
      imageOutputFormat,
      imageQuality,
      encodingHwAccel,
      overlayStrategy,
      videoAutoplay,
      videoMutedByDefault,
      videoHardwareAcceleration,
      accentColor,
      exportPath,
    };
  } catch {
    return null;
  }
}

export function readAppSettings(): AppSettings {
  if (typeof window === "undefined") {
    return DEFAULT_SETTINGS;
  }

  const rawValue = window.localStorage.getItem(SETTINGS_STORAGE_KEY);
  if (!rawValue) {
    return DEFAULT_SETTINGS;
  }

  const parsedSettings = parseSettings(rawValue);
  if (!parsedSettings) {
    window.localStorage.removeItem(SETTINGS_STORAGE_KEY);
    return DEFAULT_SETTINGS;
  }

  return parsedSettings;
}

export function writeAppSettings(settings: AppSettings): void {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
}

export function applyAccentColor(accent: AccentColor): void {
  const root = document.documentElement;
  if (accent === "yellow") {
    root.removeAttribute("data-accent");
  } else {
    root.setAttribute("data-accent", accent);
  }
}

export function clearPersistedAppClientState(): void {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.removeItem(SETTINGS_STORAGE_KEY);
  window.localStorage.removeItem(THEME_STORAGE_KEY);
  window.localStorage.removeItem(DOWNLOADER_SESSION_STORAGE_KEY);
}
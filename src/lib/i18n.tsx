import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type PropsWithChildren,
} from "react";

import { readAppSettings, writeAppSettings } from "@/lib/app-settings";
import {
  normalizeLocaleCandidate,
  type LanguagePreference,
  type ResolvedLocale,
} from "@/lib/language";
import {
  messagesByLocale,
  type TranslationKey,
  type TranslationParams,
} from "@/lib/i18n-messages";
import { getSystemLocale } from "@/lib/memories-api";

type I18nContextValue = {
  languagePreference: LanguagePreference;
  resolvedLocale: ResolvedLocale;
  t: (key: TranslationKey, params?: TranslationParams) => string;
  setLanguagePreference: (nextPreference: LanguagePreference) => void;
};

const I18nContext = createContext<I18nContextValue | null>(null);

function detectBrowserLocale(): string | null {
  if (typeof navigator === "undefined") {
    return null;
  }

  if (Array.isArray(navigator.languages) && navigator.languages.length > 0) {
    const preferred = navigator.languages.find((entry) => typeof entry === "string");
    if (preferred) {
      return preferred;
    }
  }

  return typeof navigator.language === "string" ? navigator.language : null;
}

async function detectSystemLocale(): Promise<string | null> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return null;
  }

  try {
    return await getSystemLocale();
  } catch {
    return null;
  }
}

const EXPLICIT_RESOLVED_LOCALES: LanguagePreference[] = ["en", "de", "fr", "es", "it", "pl", "nl", "pt"];

async function resolveLocale(preference: LanguagePreference): Promise<ResolvedLocale> {
  if (preference !== "system" && (EXPLICIT_RESOLVED_LOCALES as string[]).includes(preference)) {
    return preference as ResolvedLocale;
  }

  const systemLocale = await detectSystemLocale();
  if (systemLocale) {
    return normalizeLocaleCandidate(systemLocale);
  }

  const browserLocale = detectBrowserLocale();
  return normalizeLocaleCandidate(browserLocale);
}

function readInitialPreference(): LanguagePreference {
  return readAppSettings().languagePreference;
}

function interpolate(template: string, params?: TranslationParams): string {
  if (!params) {
    return template;
  }

  return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (fullMatch, paramName: string) => {
    const value = params[paramName];
    if (value === undefined) {
      return fullMatch;
    }

    return String(value);
  });
}

export function I18nProvider({ children }: PropsWithChildren) {
  const [languagePreference, setLanguagePreferenceState] = useState<LanguagePreference>(
    readInitialPreference,
  );
  const [resolvedLocale, setResolvedLocale] = useState<ResolvedLocale>("en");

  useEffect(() => {
    let isCancelled = false;

    const syncLocale = async () => {
      const nextResolvedLocale = await resolveLocale(languagePreference);
      if (!isCancelled) {
        setResolvedLocale(nextResolvedLocale);
      }
    };

    void syncLocale();

    return () => {
      isCancelled = true;
    };
  }, [languagePreference]);

  const setLanguagePreference = useCallback((nextPreference: LanguagePreference) => {
    setLanguagePreferenceState(nextPreference);

    const currentSettings = readAppSettings();
    writeAppSettings({
      ...currentSettings,
      languagePreference: nextPreference,
    });
  }, []);

  const t = useCallback(
    (key: TranslationKey, params?: TranslationParams) => {
      const dictionary = messagesByLocale[resolvedLocale];
      return interpolate(dictionary[key], params);
    },
    [resolvedLocale],
  );

  const value = useMemo<I18nContextValue>(
    () => ({
      languagePreference,
      resolvedLocale,
      t,
      setLanguagePreference,
    }),
    [languagePreference, resolvedLocale, setLanguagePreference, t],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const contextValue = useContext(I18nContext);
  if (!contextValue) {
    throw new Error("useI18n must be used within an I18nProvider");
  }

  return contextValue;
}
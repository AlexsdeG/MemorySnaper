export type LanguagePreference = "system" | "en" | "de" | "fr" | "es" | "it" | "pl" | "nl" | "pt";
export type ResolvedLocale = "en" | "de" | "fr" | "es" | "it" | "pl" | "nl" | "pt";

const EXPLICIT_LOCALES: LanguagePreference[] = ["en", "de", "fr", "es", "it", "pl", "nl", "pt"];

function extractLanguageCode(input: string): string {
  const [code = ""] = input.trim().toLowerCase().split(/[-_]/);
  return code;
}

export function parseLanguagePreference(value: string | null): LanguagePreference {
  if (value === "system" || (EXPLICIT_LOCALES as string[]).includes(value ?? "")) {
    return value as LanguagePreference;
  }

  return "system";
}

export function normalizeLocaleCandidate(candidate: string | null | undefined): ResolvedLocale {
  const code = candidate ? extractLanguageCode(candidate) : "";
  const supported: ResolvedLocale[] = ["en", "de", "fr", "es", "it", "pl", "nl", "pt"];
  if ((supported as string[]).includes(code)) {
    return code as ResolvedLocale;
  }

  return "en";
}
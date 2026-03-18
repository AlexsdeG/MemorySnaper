export type LanguagePreference = "system" | "en" | "de";
export type ResolvedLocale = "en" | "de";

function extractLanguageCode(input: string): string {
  const [code = ""] = input.trim().toLowerCase().split(/[-_]/);
  return code;
}

export function parseLanguagePreference(value: string | null): LanguagePreference {
  if (value === "en" || value === "de" || value === "system") {
    return value;
  }

  return "system";
}

export function normalizeLocaleCandidate(candidate: string | null | undefined): ResolvedLocale {
  const code = candidate ? extractLanguageCode(candidate) : "";
  if (code === "de") {
    return "de";
  }

  return "en";
}
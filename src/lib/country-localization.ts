import type { ResolvedLocale } from "@/lib/language";

const ENGLISH_ALIAS_TO_CODE: Record<string, string> = {
  "bosnia and herzegovina": "BA",
  "cape verde": "CV",
  "czechia": "CZ",
  "dr congo": "CD",
  "ivory coast": "CI",
  "north macedonia": "MK",
  "s sweden": "SE",
  "s sudan": "SS",
  "swaziland": "SZ",
  "timor leste": "TL",
  "u s virgin islands": "VI",
  "vatican": "VA",
};

let englishCountryToCodeCache: Map<string, string> | null = null;

function normalizeCountryKey(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[.'’]/g, "")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function getRegionCodes(): string[] {
  if (typeof Intl.supportedValuesOf === "function") {
    try {
      return Intl.supportedValuesOf("region");
    } catch {
      // Fallback below.
    }
  }

  // Keep a practical fallback if supportedValuesOf is unavailable.
  return [
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU",
    "IE", "IT", "LV", "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
    "GB", "NO", "CH", "IS", "UA", "US", "CA", "AU", "JP", "CN", "IN", "BR", "MX", "ZA",
  ];
}

function getEnglishCountryToCode(): Map<string, string> {
  if (englishCountryToCodeCache) {
    return englishCountryToCodeCache;
  }

  const map = new Map<string, string>();
  const display = new Intl.DisplayNames(["en"], { type: "region" });

  for (const code of getRegionCodes()) {
    const englishName = display.of(code);
    if (!englishName) {
      continue;
    }

    map.set(normalizeCountryKey(englishName), code);
  }

  for (const [alias, code] of Object.entries(ENGLISH_ALIAS_TO_CODE)) {
    map.set(alias, code);
  }

  englishCountryToCodeCache = map;
  return map;
}

export function localizeCountryName(countryName: string, locale: ResolvedLocale): string {
  const key = normalizeCountryKey(countryName);
  const code = getEnglishCountryToCode().get(key);

  if (!code) {
    return countryName;
  }

  const localized = new Intl.DisplayNames([locale], { type: "region" }).of(code);
  return localized ?? countryName;
}

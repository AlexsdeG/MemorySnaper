import type { ViewerMediaKind } from "@/lib/memories-api";
import { parseViewerDate } from "@/features/viewer/viewer-dates";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type TimeSlot = "morning" | "afternoon" | "evening" | "night";

export type ViewerFilterState = {
  searchQuery: string;
  mediaKinds: Set<ViewerMediaKind>;
  mediaFormats: Set<string>;
  dateFrom: Date | null;
  dateTo: Date | null;
  timeSlots: Set<TimeSlot>;
  locationQuery: string;
  countries: Set<string>;
  hasLocationOnly: boolean;
};

export type FilterMeta = {
  uniqueCountries: string[];
  uniqueFormats: string[];
};

// Minimal shape required from items — lets the functions work with the local
// GridItem type in ViewerPlaceholder without a circular import.
type FilterableItem = {
  dateTaken: string;
  location?: string;
  mediaKind: ViewerMediaKind;
  mediaFormat?: string;
};

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

export const DEFAULT_FILTER_STATE: ViewerFilterState = {
  searchQuery: "",
  mediaKinds: new Set(),
  mediaFormats: new Set(),
  dateFrom: null,
  dateTo: null,
  timeSlots: new Set(),
  locationQuery: "",
  countries: new Set(),
  hasLocationOnly: false,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Extracts the country from a resolved location string.
 * "Berlin, Germany" → "Germany"
 * "Tokyo" → "Tokyo"
 */
export function extractCountry(location: string): string {
  const trimmed = location.trim();
  const lastComma = trimmed.lastIndexOf(",");
  if (lastComma === -1) {
    return trimmed;
  }
  return trimmed.slice(lastComma + 1).trim();
}

/**
 * Maps a UTC hour (0–23) to a time-of-day slot.
 */
function hourToTimeSlot(hour: number): TimeSlot {
  if (hour >= 6 && hour <= 11) return "morning";
  if (hour >= 12 && hour <= 17) return "afternoon";
  if (hour >= 18 && hour <= 21) return "evening";
  return "night"; // 22–5
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Derives the unique sortable countries and media formats present in the
 * loaded item list. Should be computed once after items load.
 */
export function extractFilterMeta(items: FilterableItem[]): FilterMeta {
  const countries = new Set<string>();
  const formats = new Set<string>();

  for (const item of items) {
    if (item.location) {
      countries.add(extractCountry(item.location));
    }
    if (item.mediaFormat) {
      formats.add(item.mediaFormat.toUpperCase());
    }
  }

  return {
    uniqueCountries: [...countries].sort((a, b) => a.localeCompare(b)),
    uniqueFormats: [...formats].sort((a, b) => a.localeCompare(b)),
  };
}

/**
 * Returns the number of active filter dimensions (for the badge count).
 * Each dimension counts as 1 regardless of how many values are selected.
 */
export function countActiveFilters(filters: ViewerFilterState): number {
  let count = 0;
  if (filters.searchQuery.trim().length > 0) count += 1;
  if (filters.mediaKinds.size > 0) count += 1;
  if (filters.mediaFormats.size > 0) count += 1;
  if (filters.dateFrom !== null || filters.dateTo !== null) count += 1;
  if (filters.timeSlots.size > 0) count += 1;
  if (filters.locationQuery.trim().length > 0) count += 1;
  if (filters.countries.size > 0) count += 1;
  if (filters.hasLocationOnly) count += 1;
  return count;
}

/**
 * Pure filter function. Returns the subset of items that satisfy every active
 * filter dimension. When a dimension's filter state is "empty" (zero
 * selections / blank text / null), that dimension is considered inactive and
 * all items pass it.
 */
export function applyViewerFilters<T extends FilterableItem>(
  items: T[],
  filters: ViewerFilterState,
): T[] {
  const {
    searchQuery,
    mediaKinds,
    mediaFormats,
    dateFrom,
    dateTo,
    timeSlots,
    locationQuery,
    countries,
    hasLocationOnly,
  } = filters;

  const normalizedSearch = searchQuery.trim().toLowerCase();
  const normalizedLocationQuery = locationQuery.trim().toLowerCase();

  // Pre-compute end-of-day for dateTo so items ON the to-date are included.
  let dateToEndOfDay: Date | null = null;
  if (dateTo !== null) {
    dateToEndOfDay = new Date(dateTo);
    dateToEndOfDay.setHours(23, 59, 59, 999);
  }

  return items.filter((item) => {
    // -- Has GPS location only --
    if (hasLocationOnly && !item.location) {
      return false;
    }

    // -- Media kind (image / video) --
    if (mediaKinds.size > 0 && !mediaKinds.has(item.mediaKind)) {
      return false;
    }

    // -- Media format (MP4, WEBP, …) --
    if (mediaFormats.size > 0) {
      const normalizedFormat = item.mediaFormat?.toUpperCase() ?? "";
      if (!mediaFormats.has(normalizedFormat)) {
        return false;
      }
    }

    // -- Date range and time-of-day (requires a parseable date) --
    const parsedDate = parseViewerDate(item.dateTaken);

    if (dateFrom !== null || dateToEndOfDay !== null || timeSlots.size > 0) {
      if (!parsedDate) {
        // Items with unparseable dates fail any date-based filter.
        return false;
      }

      if (dateFrom !== null && parsedDate < dateFrom) {
        return false;
      }

      if (dateToEndOfDay !== null && parsedDate > dateToEndOfDay) {
        return false;
      }

      if (timeSlots.size > 0) {
        const utcHour = parsedDate.getUTCHours();
        const slot = hourToTimeSlot(utcHour);
        if (!timeSlots.has(slot)) {
          return false;
        }
      }
    }

    // -- Location free-text --
    if (normalizedLocationQuery.length > 0) {
      const locationText = (item.location ?? "").toLowerCase();
      if (!locationText.includes(normalizedLocationQuery)) {
        return false;
      }
    }

    // -- Country checklist --
    if (countries.size > 0) {
      if (!item.location) {
        return false;
      }
      const itemCountry = extractCountry(item.location);
      if (!countries.has(itemCountry)) {
        return false;
      }
    }

    // -- Full-text search query --
    if (normalizedSearch.length > 0) {
      const haystack = [
        item.dateTaken,
        item.location ?? "",
        item.mediaFormat ?? "",
        item.mediaKind,
      ]
        .join(" ")
        .toLowerCase();

      if (!haystack.includes(normalizedSearch)) {
        return false;
      }
    }

    return true;
  });
}

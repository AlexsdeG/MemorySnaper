import { useMemo, useState, type ReactNode } from "react";
import { CalendarIcon, Search, SlidersHorizontal, X } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@/components/ui/toggle-group";
import {
  countActiveFilters,
  DEFAULT_FILTER_STATE,
  type FilterMeta,
  type TimeSlot,
  type ViewerFilterState,
} from "@/features/viewer/viewer-filters";
import { useI18n } from "@/lib/i18n";
import type { TranslationKey } from "@/lib/i18n-messages";
import { localizeCountryName } from "@/lib/country-localization";
import { cn } from "@/lib/utils";
import type { ViewerMediaKind } from "@/lib/memories-api";

export type ViewerFilterBarProps = {
  filters: ViewerFilterState;
  onChange: (filters: ViewerFilterState) => void;
  filterMeta: FilterMeta;
  totalCount: number;
  filteredCount: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

function formatDateLabel(date: Date | null, fallbackLabel: string): string {
  if (!date) {
    return fallbackLabel;
  }

  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
  }).format(date);
}

function SectionTitle({ children }: { children: ReactNode }) {
  return <Label className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{children}</Label>;
}

const TIME_SLOT_KEYS: Record<TimeSlot, TranslationKey> = {
  morning: "viewer.filters.timeSlot.morning",
  afternoon: "viewer.filters.timeSlot.afternoon",
  evening: "viewer.filters.timeSlot.evening",
  night: "viewer.filters.timeSlot.night",
};

export function ViewerFilterBar({
  filters,
  onChange,
  filterMeta,
  totalCount,
  filteredCount,
  open,
  onOpenChange,
}: ViewerFilterBarProps) {
  const { t, resolvedLocale } = useI18n();
  const [isDateFromOpen, setIsDateFromOpen] = useState(false);
  const [isDateToOpen, setIsDateToOpen] = useState(false);

  const activeCount = useMemo(() => countActiveFilters(filters), [filters]);
  const localizedCountries = useMemo(
    () =>
      filterMeta.uniqueCountries
        .map((country) => ({
          value: country,
          label: localizeCountryName(country, resolvedLocale),
        }))
        .sort((a, b) => a.label.localeCompare(b.label, resolvedLocale)),
    [filterMeta.uniqueCountries, resolvedLocale],
  );

  const patchFilters = (patch: Partial<ViewerFilterState>) => {
    onChange({ ...filters, ...patch });
  };

  return (
    <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
      {/* Inline search */}
      <div className="relative flex-1">
        <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          type="search"
          placeholder={t("viewer.filters.searchPlaceholder")}
          aria-label={t("viewer.filters.searchPlaceholder")}
          className="pl-8"
          value={filters.searchQuery}
          onChange={(event) => {
            patchFilters({ searchQuery: event.target.value });
          }}
        />
      </div>

      {/* Sheet trigger */}
      <Button
        type="button"
        variant="outline"
        className="gap-1.5"
        onClick={() => { onOpenChange(true); }}
      >
        <SlidersHorizontal className="size-4" />
        {t("viewer.filters.filtersButton")}
        {activeCount > 0 ? (
          <Badge variant="secondary" className="ml-0.5 px-1.5">
            {activeCount}
          </Badge>
        ) : null}
      </Button>

      {/* Filter Sheet */}
      <Sheet open={open} onOpenChange={onOpenChange}>
        <SheetContent side="left" className="flex h-full w-80 flex-col overflow-y-auto sm:w-96">
          <SheetHeader>
            <SheetTitle>{t("viewer.filters.filtersButton")}</SheetTitle>
            <SheetDescription>
              {t("viewer.filters.resultsCount", { filtered: filteredCount, total: totalCount })}
            </SheetDescription>
          </SheetHeader>

          <div className="flex-1 overflow-y-auto px-4 py-3 sm:px-5">
            <div className="space-y-5 pb-4">
              {/* Media Type */}
              <div className="space-y-2">
                <SectionTitle>{t("viewer.filters.mediaType")}</SectionTitle>
                <ToggleGroup
                  type="multiple"
                  size="sm"
                  variant="outline"
                  value={[...filters.mediaKinds]}
                  onValueChange={(value) => {
                    patchFilters({ mediaKinds: new Set(value as ViewerMediaKind[]) });
                  }}
                >
                  <ToggleGroupItem value="image">
                    {t("viewer.filters.mediaType.image")}
                  </ToggleGroupItem>
                  <ToggleGroupItem value="video">
                    {t("viewer.filters.mediaType.video")}
                  </ToggleGroupItem>
                </ToggleGroup>
              </div>

              <Separator />

              {/* Media Format */}
              <div className="space-y-2">
                <SectionTitle>{t("viewer.filters.mediaFormat")}</SectionTitle>
                {filterMeta.uniqueFormats.length === 0 ? (
                  <p className="text-xs text-muted-foreground">{t("viewer.filters.noFormatMetadata")}</p>
                ) : (
                  <div className="grid gap-1">
                    {filterMeta.uniqueFormats.map((format) => (
                      <Label key={format} className="text-sm font-normal">
                        <Checkbox
                          checked={filters.mediaFormats.has(format)}
                          onCheckedChange={() => {
                            patchFilters({
                              mediaFormats: new Set(
                                filters.mediaFormats.has(format)
                                  ? [...filters.mediaFormats].filter((f) => f !== format)
                                  : [...filters.mediaFormats, format],
                              ),
                            });
                          }}
                        />
                        {format}
                      </Label>
                    ))}
                  </div>
                )}
              </div>

              <Separator />

              {/* Date Range */}
              <div className="space-y-2">
                <SectionTitle>{t("viewer.filters.dateRange")}</SectionTitle>
                <div className="space-y-2">
                  <div className="space-y-1">
                    <Label className="text-xs text-muted-foreground">{t("viewer.filters.dateFrom")}</Label>
                    <Popover open={isDateFromOpen} onOpenChange={setIsDateFromOpen}>
                      <PopoverTrigger asChild>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          className="w-full justify-start"
                          aria-label={t("viewer.filters.dateFrom")}
                        >
                          <CalendarIcon className="mr-2 size-4" />
                          {formatDateLabel(filters.dateFrom, t("viewer.filters.selectDate"))}
                        </Button>
                      </PopoverTrigger>
                      <PopoverContent className="w-auto p-0" align="start">
                        <Calendar
                          mode="single"
                          selected={filters.dateFrom ?? undefined}
                          onSelect={(date) => {
                            patchFilters({ dateFrom: date ?? null });
                            setIsDateFromOpen(false);
                          }}
                          initialFocus
                        />
                      </PopoverContent>
                    </Popover>
                  </div>
                  <div className="space-y-1">
                    <Label className="text-xs text-muted-foreground">{t("viewer.filters.dateTo")}</Label>
                    <Popover open={isDateToOpen} onOpenChange={setIsDateToOpen}>
                      <PopoverTrigger asChild>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          className="w-full justify-start"
                          aria-label={t("viewer.filters.dateTo")}
                        >
                          <CalendarIcon className="mr-2 size-4" />
                          {formatDateLabel(filters.dateTo, t("viewer.filters.selectDate"))}
                        </Button>
                      </PopoverTrigger>
                      <PopoverContent className="w-auto p-0" align="start">
                        <Calendar
                          mode="single"
                          selected={filters.dateTo ?? undefined}
                          onSelect={(date) => {
                            patchFilters({ dateTo: date ?? null });
                            setIsDateToOpen(false);
                          }}
                          initialFocus
                        />
                      </PopoverContent>
                    </Popover>
                  </div>
                  {(filters.dateFrom || filters.dateTo) ? (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="w-full justify-start text-muted-foreground"
                      onClick={() => {
                        patchFilters({ dateFrom: null, dateTo: null });
                      }}
                    >
                      <X className="mr-2 size-3.5" />
                      {t("viewer.filters.clearFilters")}
                    </Button>
                  ) : null}
                </div>
              </div>

              <Separator />

              {/* Time of Day */}
              <div className="space-y-2">
                <SectionTitle>{t("viewer.filters.timeOfDay")}</SectionTitle>
                <ToggleGroup
                  type="multiple"
                  size="sm"
                  variant="outline"
                  className="flex-wrap"
                  value={[...filters.timeSlots]}
                  onValueChange={(value) => {
                    patchFilters({ timeSlots: new Set(value as TimeSlot[]) });
                  }}
                >
                  {(["morning", "afternoon", "evening", "night"] as TimeSlot[]).map((slot) => (
                    <ToggleGroupItem key={slot} value={slot}>
                      {t(TIME_SLOT_KEYS[slot])}
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
              </div>

              <Separator />

              {/* Location */}
              <div className="space-y-2">
                <SectionTitle>{t("viewer.filters.location")}</SectionTitle>
                <Input
                  placeholder={t("viewer.filters.locationPlaceholder")}
                  aria-label={t("viewer.filters.locationPlaceholder")}
                  value={filters.locationQuery}
                  onChange={(event) => {
                    patchFilters({ locationQuery: event.target.value });
                  }}
                />
                <Label className="text-sm font-normal">
                  <Checkbox
                    checked={filters.hasLocationOnly}
                    onCheckedChange={(checked) => {
                      patchFilters({ hasLocationOnly: checked === true });
                    }}
                  />
                  {t("viewer.filters.hasLocationOnly")}
                </Label>
              </div>

              <Separator />

              {/* Countries */}
              <div className="space-y-2">
                <SectionTitle>{t("viewer.filters.countries")}</SectionTitle>
                {localizedCountries.length === 0 ? (
                  <p className="text-xs text-muted-foreground">{t("viewer.filters.noCountryMetadata")}</p>
                ) : (
                  <div className="grid gap-1">
                    {localizedCountries.map((country) => (
                      <Label key={country.value} className="text-sm font-normal">
                        <Checkbox
                          checked={filters.countries.has(country.value)}
                          onCheckedChange={() => {
                            patchFilters({
                              countries: new Set(
                                filters.countries.has(country.value)
                                  ? [...filters.countries].filter((c) => c !== country.value)
                                  : [...filters.countries, country.value],
                              ),
                            });
                          }}
                        />
                        {country.label}
                      </Label>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>

          <SheetFooter className="flex-row items-center gap-2 border-t pt-4">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                onChange({
                  ...DEFAULT_FILTER_STATE,
                  mediaKinds: new Set(),
                  mediaFormats: new Set(),
                  timeSlots: new Set(),
                  countries: new Set(),
                });
              }}
              disabled={activeCount === 0}
              className="gap-1"
            >
              <X className="size-3.5" />
              {t("viewer.filters.clearFilters")}
            </Button>

            <span className={cn("ml-auto text-xs text-muted-foreground", activeCount > 0 && "text-foreground")}>
              {t("viewer.filters.resultsCount", { filtered: filteredCount, total: totalCount })}
            </span>

            <SheetClose asChild>
              <Button type="button" size="sm">
                {t("viewer.filters.done")}
              </Button>
            </SheetClose>
          </SheetFooter>
        </SheetContent>
      </Sheet>
    </div>
  );
}

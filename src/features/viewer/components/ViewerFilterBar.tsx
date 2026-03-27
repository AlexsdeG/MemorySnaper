import { useMemo, useState, type ReactNode } from "react";
import { CalendarIcon, ChevronDown, ChevronUp, Search, X } from "lucide-react";

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
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import {
  countActiveFilters,
  DEFAULT_FILTER_STATE,
  type FilterMeta,
  type TimeSlot,
  type ViewerFilterState,
} from "@/features/viewer/viewer-filters";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export type ViewerFilterBarProps = {
  filters: ViewerFilterState;
  onChange: (filters: ViewerFilterState) => void;
  filterMeta: FilterMeta;
  totalCount: number;
  filteredCount: number;
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

function toggleSetValue<T>(source: Set<T>, value: T): Set<T> {
  const next = new Set(source);
  if (next.has(value)) {
    next.delete(value);
  } else {
    next.add(value);
  }
  return next;
}

function SectionTitle({ children }: { children: ReactNode }) {
  return <Label className="text-xs text-muted-foreground">{children}</Label>;
}

export function ViewerFilterBar({
  filters,
  onChange,
  filterMeta,
  totalCount,
  filteredCount,
}: ViewerFilterBarProps) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);

  const activeCount = useMemo(() => countActiveFilters(filters), [filters]);

  const patchFilters = (patch: Partial<ViewerFilterState>) => {
    onChange({ ...filters, ...patch });
  };

  return (
    <div className="space-y-3 rounded-md border border-border p-3">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="search"
            placeholder={t("viewer.filters.searchPlaceholder")}
            className="pl-8"
            value={filters.searchQuery}
            onChange={(event) => {
              patchFilters({ searchQuery: event.target.value });
            }}
          />
        </div>

        <Button
          type="button"
          variant="outline"
          onClick={() => {
            setExpanded((previous) => !previous);
          }}
        >
          {t("viewer.filters.filtersButton")}
          {activeCount > 0 ? (
            <Badge variant="secondary" className="ml-1 px-1.5">
              {activeCount}
            </Badge>
          ) : null}
          {expanded ? <ChevronUp className="ml-1 size-4" /> : <ChevronDown className="ml-1 size-4" />}
        </Button>
      </div>

      {expanded ? (
        <>
          <Separator />

          <div className="grid gap-4 lg:grid-cols-2 xl:grid-cols-3">
            <div className="space-y-2">
              <SectionTitle>{t("viewer.filters.mediaType")}</SectionTitle>
              <div className="flex flex-wrap gap-2">
                {(["image", "video"] as const).map((mediaKind) => {
                  const selected = filters.mediaKinds.has(mediaKind);
                  return (
                    <Button
                      key={mediaKind}
                      type="button"
                      variant={selected ? "default" : "outline"}
                      size="sm"
                      onClick={() => {
                        patchFilters({
                          mediaKinds: toggleSetValue(filters.mediaKinds, mediaKind),
                        });
                      }}
                    >
                      {mediaKind === "image"
                        ? t("viewer.filters.mediaType.image")
                        : t("viewer.filters.mediaType.video")}
                    </Button>
                  );
                })}
              </div>
            </div>

            <div className="space-y-2">
              <SectionTitle>{t("viewer.filters.dateRange")}</SectionTitle>
              <div className="flex flex-wrap gap-2">
                <Popover>
                  <PopoverTrigger asChild>
                    <Button type="button" variant="outline" size="sm" className="min-w-40 justify-start">
                      <CalendarIcon className="mr-2 size-4" />
                      {`${t("viewer.filters.dateFrom")}: ${formatDateLabel(filters.dateFrom, t("viewer.filters.selectDate"))}`}
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent align="start" className="w-auto p-0">
                    <Calendar
                      mode="single"
                      selected={filters.dateFrom ?? undefined}
                      onSelect={(date) => {
                        patchFilters({ dateFrom: date ?? null });
                      }}
                    />
                  </PopoverContent>
                </Popover>

                <Popover>
                  <PopoverTrigger asChild>
                    <Button type="button" variant="outline" size="sm" className="min-w-40 justify-start">
                      <CalendarIcon className="mr-2 size-4" />
                      {`${t("viewer.filters.dateTo")}: ${formatDateLabel(filters.dateTo, t("viewer.filters.selectDate"))}`}
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent align="start" className="w-auto p-0">
                    <Calendar
                      mode="single"
                      selected={filters.dateTo ?? undefined}
                      onSelect={(date) => {
                        patchFilters({ dateTo: date ?? null });
                      }}
                    />
                  </PopoverContent>
                </Popover>
              </div>
            </div>

            <div className="space-y-2">
              <SectionTitle>{t("viewer.filters.timeOfDay")}</SectionTitle>
              <div className="flex flex-wrap gap-2">
                {(["morning", "afternoon", "evening", "night"] as TimeSlot[]).map((slot) => {
                  const selected = filters.timeSlots.has(slot);
                  return (
                    <Button
                      key={slot}
                      type="button"
                      variant={selected ? "default" : "outline"}
                      size="sm"
                      onClick={() => {
                        patchFilters({
                          timeSlots: toggleSetValue(filters.timeSlots, slot),
                        });
                      }}
                    >
                      {t(`viewer.filters.timeSlot.${slot}`)}
                    </Button>
                  );
                })}
              </div>
            </div>

            <div className="space-y-2">
              <SectionTitle>{t("viewer.filters.location")}</SectionTitle>
              <div className="space-y-2">
                <Input
                  placeholder={t("viewer.filters.locationPlaceholder")}
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
            </div>

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
                            mediaFormats: toggleSetValue(filters.mediaFormats, format),
                          });
                        }}
                      />
                      {format}
                    </Label>
                  ))}
                </div>
              )}
            </div>

            <div className="space-y-2">
              <SectionTitle>{t("viewer.filters.countries")}</SectionTitle>
              {filterMeta.uniqueCountries.length === 0 ? (
                <p className="text-xs text-muted-foreground">{t("viewer.filters.noCountryMetadata")}</p>
              ) : (
                <ScrollArea className="h-36 rounded-md border border-border p-2">
                  <div className="grid gap-1 pr-2">
                    {filterMeta.uniqueCountries.map((country) => (
                      <Label key={country} className="text-sm font-normal">
                        <Checkbox
                          checked={filters.countries.has(country)}
                          onCheckedChange={() => {
                            patchFilters({
                              countries: toggleSetValue(filters.countries, country),
                            });
                          }}
                        />
                        {country}
                      </Label>
                    ))}
                  </div>
                </ScrollArea>
              )}
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
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
            >
              <X className="mr-1 size-4" />
              {t("viewer.filters.clearFilters")}
            </Button>

            <span className={cn("text-xs text-muted-foreground", activeCount > 0 && "text-foreground")}>
              {t("viewer.filters.resultsCount", { filtered: filteredCount, total: totalCount })}
            </span>
          </div>
        </>
      ) : null}
    </div>
  );
}

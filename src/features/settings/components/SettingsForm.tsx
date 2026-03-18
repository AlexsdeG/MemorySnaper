import { useEffect, useMemo, useState, type ChangeEvent } from "react";
import { useTheme } from "next-themes";
import { Button } from "@/components/ui/button";

const REQUESTS_WARNING_THRESHOLD = 100;
const CONCURRENCY_WARNING_THRESHOLD = 5;
const SETTINGS_STORAGE_KEY = "memorysnaper.rate-limit-settings";

type RateLimitSettings = {
  requestsPerMinute: number;
  concurrentDownloads: number;
};

function clampNonNegativeInteger(value: string): number {
  const parsedValue = Number.parseInt(value, 10);
  if (Number.isNaN(parsedValue) || parsedValue < 0) {
    return 0;
  }

  return parsedValue;
}

type ThemeOption = "light" | "dark" | "system";

export function SettingsForm() {
  const { theme, setTheme } = useTheme();
  const [requestsPerMinute, setRequestsPerMinute] = useState<number>(60);
  const [concurrentDownloads, setConcurrentDownloads] = useState<number>(3);

  useEffect(() => {
    const rawValue = window.localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (!rawValue) {
      return;
    }

    try {
      const parsedValue: unknown = JSON.parse(rawValue);
      if (!parsedValue || typeof parsedValue !== "object") {
        return;
      }

      const storedRequests = Reflect.get(parsedValue, "requestsPerMinute");
      const storedConcurrent = Reflect.get(parsedValue, "concurrentDownloads");

      if (typeof storedRequests === "number" && Number.isFinite(storedRequests)) {
        setRequestsPerMinute(Math.max(0, Math.floor(storedRequests)));
      }

      if (typeof storedConcurrent === "number" && Number.isFinite(storedConcurrent)) {
        setConcurrentDownloads(Math.max(0, Math.floor(storedConcurrent)));
      }
    } catch {
      window.localStorage.removeItem(SETTINGS_STORAGE_KEY);
    }
  }, []);

  useEffect(() => {
    const settings: RateLimitSettings = {
      requestsPerMinute,
      concurrentDownloads,
    };

    window.localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  }, [concurrentDownloads, requestsPerMinute]);

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

  return (
    <form className="space-y-4" onSubmit={(event) => event.preventDefault()}>
      <div className="space-y-2">
        <p className="text-sm font-medium">Appearance</p>
        <div className="flex gap-2">
          {(["light", "system", "dark"] as ThemeOption[]).map((option) => (
            <Button
              key={option}
              type="button"
              variant={theme === option ? "default" : "outline"}
              className="flex-1 capitalize"
              onClick={() => setTheme(option)}
            >
              {option}
            </Button>
          ))}
        </div>
      </div>
      <div className="space-y-2">
        <label htmlFor="requests-per-minute" className="text-sm font-medium">
          Requests per Minute
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
          Concurrent Downloads
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

      {showWarning ? (
        <p className="text-sm text-red-600">
          Warning: Values above 100 RPM or 5 concurrent downloads may trigger throttling.
        </p>
      ) : null}
    </form>
  );
}

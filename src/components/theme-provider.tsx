import { ThemeProvider as NextThemesProvider, useTheme } from "next-themes";
import { useEffect, type ComponentProps } from "react";

type ThemeMode = "light" | "dark";

function applyTheme(theme: ThemeMode): void {
  const root = document.documentElement;
  root.classList.toggle("dark", theme === "dark");
  root.style.colorScheme = theme;
}

function extractThemeMode(value: unknown): ThemeMode | null {
  if (value === "dark" || value === "light") {
    return value;
  }

  return null;
}

function detectBrowserTheme(): ThemeMode {
  if (typeof window.matchMedia !== "function") {
    return "light";
  }

  try {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  } catch {
    return "light";
  }
}

async function detectTauriTheme(): Promise<ThemeMode | null> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return null;
  }

  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    return extractThemeMode(await getCurrentWindow().theme());
  } catch {
    return null;
  }
}

function ThemeSystemBridge() {
  const { theme } = useTheme();

  useEffect(() => {
    if (theme !== "system") {
      return;
    }

    let cleanupTauriThemeListener: (() => void) | undefined;
    let mediaQueryList: MediaQueryList | null = null;

    const applySystemTheme = async () => {
      const nextTheme = (await detectTauriTheme()) ?? detectBrowserTheme();
      applyTheme(nextTheme);
    };

    const mediaQueryHandler = (event: MediaQueryListEvent) => {
      applyTheme(event.matches ? "dark" : "light");
    };

    void applySystemTheme();

    if (typeof window.matchMedia === "function") {
      mediaQueryList = window.matchMedia("(prefers-color-scheme: dark)");
      if (typeof mediaQueryList.addEventListener === "function") {
        mediaQueryList.addEventListener("change", mediaQueryHandler);
      } else {
        mediaQueryList.addListener(mediaQueryHandler);
      }
    }

    if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
      void (async () => {
        try {
          const { getCurrentWindow } = await import("@tauri-apps/api/window");
          cleanupTauriThemeListener = await getCurrentWindow().onThemeChanged((event: unknown) => {
            const payload =
              typeof event === "object" && event !== null && "payload" in event
                ? Reflect.get(event, "payload")
                : event;
            const nextTheme = extractThemeMode(payload);
            if (nextTheme) {
              applyTheme(nextTheme);
            }
          });
        } catch {
          cleanupTauriThemeListener = undefined;
        }
      })();
    }

    return () => {
      if (mediaQueryList) {
        if (typeof mediaQueryList.removeEventListener === "function") {
          mediaQueryList.removeEventListener("change", mediaQueryHandler);
        } else {
          mediaQueryList.removeListener(mediaQueryHandler);
        }
      }
      cleanupTauriThemeListener?.();
    };
  }, [theme]);

  return null;
}

export function ThemeProvider({ children, ...props }: ComponentProps<typeof NextThemesProvider>) {
  return (
    <NextThemesProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      storageKey="memorysnaper-theme"
      {...props}
    >
      <ThemeSystemBridge />
      {children}
    </NextThemesProvider>
  );
}

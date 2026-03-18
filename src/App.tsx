import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { DownloaderPlaceholder } from "@/features/downloader/components/DownloaderPlaceholder";
import { SettingsPlaceholder } from "@/features/settings/components/SettingsPlaceholder";
import { ViewerPlaceholder } from "@/features/viewer/components/ViewerPlaceholder";

type TabKey = "downloader" | "viewer" | "settings";

const tabs: Array<{ key: TabKey; label: string }> = [
  { key: "downloader", label: "Downloader" },
  { key: "viewer", label: "Viewer" },
  { key: "settings", label: "Settings" },
];

function App() {
  const [activeTab, setActiveTab] = useState<TabKey>("downloader");

  const tabContent = useMemo(() => {
    switch (activeTab) {
      case "downloader":
        return {
          title: "Downloader",
          component: <DownloaderPlaceholder />,
        };
      case "viewer":
        return {
          title: "Viewer",
          component: <ViewerPlaceholder />,
        };
      case "settings":
        return {
          title: "Settings",
          component: <SettingsPlaceholder />,
        };
      default:
        return {
          title: "Downloader",
          component: <DownloaderPlaceholder />,
        };
    }
  }, [activeTab]);

  return (
    <div className="flex h-screen w-full flex-col bg-background text-foreground">
      {/* Step 1.2 — Tab bar: fixed bottom on mobile, relative top on desktop */}
      <nav className="fixed bottom-0 w-full z-50 border-t bg-background md:relative md:top-0 md:border-b md:border-t-0">
        <div className="mx-auto flex w-full max-w-4xl items-center gap-2 px-4 py-3">
          {tabs.map((tab) => (
            <Button
              key={tab.key}
              type="button"
              variant={activeTab === tab.key ? "default" : "outline"}
              className="flex-1"
              onClick={() => setActiveTab(tab.key)}
            >
              {tab.label}
            </Button>
          ))}
        </div>
      </nav>

      {/* Step 1.3 — Scrollable content area; pb-16 prevents mobile tab bar overlap */}
      <div className="flex-1 overflow-y-auto pb-16 md:pb-0">
        <main className="mx-auto w-full max-w-4xl px-4 py-6">
          <header className="mb-6">
            <h1 className="text-2xl font-semibold tracking-tight">MemorySnaper</h1>
            <p className="text-sm text-muted-foreground">Phase 1 tab layout scaffold</p>
          </header>

          <section aria-label={tabContent.title}>{tabContent.component}</section>
        </main>
      </div>
    </div>
  );
}

export default App;

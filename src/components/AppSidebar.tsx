import { useState } from "react";
import { Download, Images, Settings, Camera, CircleHelp } from "lucide-react";

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";
import { useI18n } from "@/lib/i18n";
import { GuideListSheet } from "@/components/GuideListSheet";

export type TabKey = "downloader" | "viewer" | "settings";

interface AppSidebarProps {
  activeTab: TabKey;
  onTabChange: (tab: TabKey) => void;
}

const NAV_ICONS: Record<TabKey, React.ComponentType<{ className?: string }>> = {
  downloader: Download,
  viewer: Images,
  settings: Settings,
};

export function AppSidebar({ activeTab, onTabChange }: AppSidebarProps) {
  const { t } = useI18n();
  const { state } = useSidebar();
  const [helpOpen, setHelpOpen] = useState(false);

  const navItems: Array<{ key: TabKey; label: string }> = [
    { key: "downloader", label: t("app.tabs.downloader") },
    { key: "viewer", label: t("app.tabs.viewer") },
    { key: "settings", label: t("app.tabs.settings") },
  ];

  return (
    <Sidebar collapsible="icon" variant="sidebar">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" className="pointer-events-none">
              <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
                <Camera className="size-4" />
              </div>
              {state === "expanded" && (
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-semibold">MemorySnaper</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {t("app.header.tagline")}
                  </span>
                </div>
              )}
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu className="gap-2">
              {navItems.map((item) => {
                const Icon = NAV_ICONS[item.key];
                return (
                  <SidebarMenuItem key={item.key}>
                    <SidebarMenuButton
                      isActive={activeTab === item.key}
                      tooltip={item.label}
                      onClick={() => onTabChange(item.key)}
                    >
                      <Icon className="size-4" />
                      <span>{item.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                );
              })}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu className="gap-2">
          <SidebarMenuItem>
            <SidebarMenuButton
              tooltip={t("app.sidebar.help")}
              onClick={() => setHelpOpen(true)}
            >
              <CircleHelp className="size-4" />
              <span>{t("app.sidebar.help")}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarTrigger className="w-full justify-start p-2" />
          </SidebarMenuItem>
        </SidebarMenu>
        <GuideListSheet open={helpOpen} onOpenChange={setHelpOpen} />
      </SidebarFooter>
    </Sidebar>
  );
}

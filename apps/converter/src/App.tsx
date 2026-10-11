import { useEffect, useState } from "react";
import { Toaster } from "sonner";
import { invoke } from "@tauri-apps/api/core";
import Convert from "@frost/shared/Components/Convert/Convert";
import WindowControls, { IS_MAC } from "@frost/shared/Components/Shell/WindowControls";
import { ThemeProvider, useTheme } from "@frost/shared/Context/Theme";
import { cn } from "@frost/shared/lib/utils";
import { I18nProvider } from "@/i18n";
import { UpdateBanner, UpdateProvider } from "./Update";

/** The Studio's title bar silhouette, with only the name in it. */
function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      className={cn("flex h-[52px] flex-none select-none items-center border-b border-border bg-window", IS_MAC ? "pl-[82px] pr-4" : "pl-[18px]")}
    >
      <div data-tauri-drag-region className="flex shrink-0 flex-col justify-center leading-none">
        <span className="headline text-[16px] leading-none">Converter</span>
        <span className="mt-0.5 text-[10px] leading-none text-muted-foreground">by mxbsecure</span>
      </div>
      <div data-tauri-drag-region className="min-w-4 flex-1 self-stretch" />
      <WindowControls className="ml-4" />
    </div>
  );
}

function ThemedToaster() {
  const { resolved } = useTheme();
  return <Toaster position="bottom-right" theme={resolved} richColors />;
}

/** Frost Converter: the Studio's Convert screen as a window of its own. */
export default function App() {
  // FBX files dropped on the exe's icon.
  const [opened, setOpened] = useState<string[]>([]);
  useEffect(() => {
    invoke<string[]>("converter_opened")
      .then(setOpened)
      .catch(() => {});
  }, []);
  return (
    <ThemeProvider defaultTheme="dark">
      <I18nProvider>
        <UpdateProvider>
          <div className="flex h-screen flex-col bg-background text-foreground">
            <TitleBar />
            <UpdateBanner />
            <div className="min-h-0 flex-1 bg-background">
              <Convert active storageKey="converter.convert" opened={opened} />
            </div>
          </div>
          <ThemedToaster />
        </UpdateProvider>
      </I18nProvider>
    </ThemeProvider>
  );
}

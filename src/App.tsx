import { useCallback, useEffect, useState } from "react";
import TitleBar from "./Components/TitleBar/TitleBar";
import Dashboard from "./Components/Dashboard/Dashboard";
import Setup from "./Components/Setup/Setup";
import Welcome from "./Components/Welcome/Welcome";
import LooseSwapPrompt from "./Components/Locker/LooseSwapPrompt";
import { ThemeProvider } from "./Context/Theme";
import { FrostmodProvider } from "./Context/Frostmod";
import { ConfigContext } from "./Context/Config";
import { Toaster } from "@/Components/ui/sonner";
import { TooltipProvider } from "@/Components/ui/tooltip";
import { getConfig, isConfigured } from "./api/mods";
import { UpdateProvider } from "./Context/Update";
import UpdateBanner from "./Components/UpdateBanner/UpdateBanner";
import type { Config } from "./types";

/** Bumped when the intro tour changes enough to warrant showing it again. */
const WELCOME_SEEN_KEY = "mxb:welcomeSeen:v1";

const App = () => {
  const [ready, setReady] = useState(false);
  const [config, setConfig] = useState<Config | null>(null);
  const [showWelcome, setShowWelcome] = useState(
    () => localStorage.getItem(WELCOME_SEEN_KEY) !== "1",
  );

  const dismissWelcome = useCallback(() => {
    localStorage.setItem(WELCOME_SEEN_KEY, "1");
    setShowWelcome(false);
  }, []);

  const reloadConfig = useCallback(async () => {
    setConfig(await getConfig());
  }, []);

  useEffect(() => {
    (async () => {
      try {
        if (await isConfigured()) await reloadConfig();
      } catch (err) {
        console.error("Startup failed", err);
      } finally {
        setReady(true);
      }
    })();
  }, [reloadConfig]);

  // Block the webview's browser refresh/find shortcuts.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (
        (e.ctrlKey && (e.code === "KeyF" || e.code === "KeyR")) ||
        e.code === "F5"
      ) {
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <ThemeProvider>
      <FrostmodProvider>
        <TooltipProvider delayDuration={300}>
          <UpdateProvider>
            <div className="grid h-screen grid-rows-[42px_auto_1fr] overflow-hidden">
              <TitleBar />
              <UpdateBanner />
              <main className="min-h-0 overflow-hidden bg-background text-foreground">
                {ready &&
                  (config ? (
                    <ConfigContext.Provider value={{ config, reloadConfig }}>
                      <Dashboard />
                    </ConfigContext.Provider>
                  ) : (
                    <Setup onComplete={reloadConfig} />
                  ))}
              </main>
            </div>
            <Toaster />
            {ready && showWelcome && <Welcome onDone={dismissWelcome} />}
            {/* Offer to register loose model-swap folders once the app is set up and the
                intro tour is out of the way. */}
            {ready && config && !showWelcome && <LooseSwapPrompt />}
          </UpdateProvider>
        </TooltipProvider>
      </FrostmodProvider>
    </ThemeProvider>
  );
};

export default App;

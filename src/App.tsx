import { CssBaseline, useMediaQuery } from "@mui/material";
import { ThemeProvider } from "@mui/material/styles";
import { useEffect, useMemo, useState } from "react";
import LoginPage from "./Components/LoginPage/LoginPage";
import "./App.scss";
import Footer from "./Components/Footer/Footer";
import TitleBar from "./Components/TitleBar/TitleBar";
import { darkTheme, lightTheme } from "./theme";
import Dashboard from "./Components/Dashboard/Dashboard";
import { ConfigContext } from "./Context/Config";
import { getConfig, isConfigured } from "./api/mods";
import type { Config } from "./types";

const App = () => {
  const [ready, setReady] = useState(false);
  const [config, setConfig] = useState<Config | null>(null);

  const loadConfig = async () => {
    setConfig(await getConfig());
  };

  useEffect(() => {
    (async () => {
      try {
        if (await isConfigured()) await loadConfig();
      } catch (err) {
        console.error("Startup failed", err);
      } finally {
        setReady(true);
      }
    })();
  }, []);

  const prefersDark = useMediaQuery("(prefers-color-scheme: dark)");
  const [isDarkMode, setIsDarkMode] = useState<boolean>(() => {
    const stored = localStorage.getItem("frost-theme");
    return stored ? stored === "dark" : prefersDark;
  });
  const theme = useMemo(
    () => (isDarkMode ? darkTheme : lightTheme),
    [isDarkMode],
  );

  const toggleTheme = () =>
    setIsDarkMode((v) => {
      const next = !v;
      localStorage.setItem("frost-theme", next ? "dark" : "light");
      return next;
    });

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
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <TitleBar isDark={isDarkMode} onToggleTheme={toggleTheme} />
      {ready &&
        (config ? (
          <ConfigContext.Provider value={config}>
            <Dashboard />
          </ConfigContext.Provider>
        ) : (
          <LoginPage onComplete={loadConfig} />
        ))}
      <Footer />
    </ThemeProvider>
  );
};

export default App;

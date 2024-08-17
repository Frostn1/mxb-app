import { CssBaseline, useMediaQuery } from "@mui/material";
import { ThemeProvider } from "@mui/material/styles";
import Dashboard from "./Components/Dashboard/Dashboard";
import TitleBar from "./Components/TitleBar/TitleBar";
import { darkTheme, lightTheme } from "./theme";
import "./App.scss";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";

const App = () => {
  console.log("Environment ", process.env.NODE_ENV);
  console.log("B-end PATH ", import.meta.env.VITE_BE_IP);

  const [isConfigured, setIsConfigured] = useState(false);
  const [isDarkMode, setIsDarkMode] = useState(
    useMediaQuery("(prefers-color-scheme: dark)"),
  );

  async function handleStartup() {
    setIsConfigured(await invoke("is_configured", {}));
  }
  useEffect(() => {
    handleStartup();
  }, []);

  window.addEventListener("keydown", function (e) {
    if (
      (e.ctrlKey && e.code == "KeyF") ||
      (e.ctrlKey && e.code == "KeyR") ||
      e.code == "F5"
    ) {
      e.preventDefault();
    }
  });
  // window.addEventListener("contextmenu", function (e) {
  //   e.preventDefault();
  // });
  return (
    <ThemeProvider theme={isDarkMode ? darkTheme : lightTheme}>
      <CssBaseline />
      <TitleBar handleChangeTheme={setIsDarkMode} />
      <Dashboard isConfigured={isConfigured} />
    </ThemeProvider>
  );
};

export default App;

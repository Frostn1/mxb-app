import { CssBaseline, useMediaQuery } from "@mui/material";
import { ThemeProvider } from "@mui/material/styles";
import { invoke } from "@tauri-apps/api";
import { useContext, useEffect, useState } from "react";
import LoginPage from "./Components/LoginPage/LoginPage";
import "./App.scss";
import Footer from "./Components/Footer/Footer";
import TitleBar from "./Components/TitleBar/TitleBar";
import { darkTheme, lightTheme } from "./theme";
import Dashboard from "./Components/Dashboard/Dashboard";
import { ConfigContext } from "./Context/Config";

const App = () => {
  console.log("Environment ", process.env.NODE_ENV);
  console.log("B-end PATH ", import.meta.env.VITE_BE_IP);

  const [isConfigured, setIsConfigured] = useState(false);
  const [config, setConfig] = useState({});

  async function handleStartup() {
    const isConfig = await invoke("is_configured", {});
    setIsConfigured(isConfig);
    return isConfig;
  }
  async function handleLogin() {
    setIsConfigured(true);
    setConfig(await invoke("get_config", {}));
  }
  useEffect(() => {
    if (handleStartup()) handleLogin();
  }, []);

  const [isDarkMode, setIsDarkMode] = useState(
    useMediaQuery("(prefers-color-scheme: dark)"),
  );

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
      {!isConfigured ? (
        <LoginPage key={"login-page"} login={handleLogin} />
      ) : (
        <ConfigContext.Provider value={config}>
          <Dashboard config={config} />
        </ConfigContext.Provider>
      )}
      <Footer />
    </ThemeProvider>
  );
};

export default App;

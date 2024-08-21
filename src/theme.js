import { createTheme } from "@mui/material/styles";

const darkTheme = createTheme({
  typography: {
    primary: "#fff",
    secondary: "rgba(255, 255, 255, 0.7)",
    disabled: "rgba(255, 255, 255, 0.5)",
  },
  palette: {
    mode: "dark",
    text: {
      primary: "#fff",
      secondary: "rgba(255, 255, 255, 0.7)",
      disabled: "rgba(255, 255, 255, 0.5)",
    },
    background: {
      default: "#121212",
      paper: "#121212",
      secondary: "#323232",
    },
    icon: {
      color: "#90DEF1",
    },
  },
  shape: {
    borderRadius: 10,
  },
});

const lightTheme = createTheme({
  typography: {
    primary: "#000",
    secondary: "rgba(0, 0, 0, 0.7)",
    disabled: "rgba(0, 0, 0, 0.5)",
  },
  palette: {
    mode: "light",
    text: {
      primary: "#000",
      secondary: "rgba(0, 0, 0, 0.7)",
      disabled: "rgba(0, 0, 0, 0.5)",
    },
    background: {
      default: "#dee4e7",
      paper: "#dee4e7",
      secondary: "#bec4c7",
    },
    icon: {
      color: "#90DEF1",
    },
  },
  shape: {
    borderRadius: 10,
  },
});

export { darkTheme, lightTheme };

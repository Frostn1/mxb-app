import { createTheme } from "@mui/material/styles";

// Extend MUI's palette with the custom `background.secondary` used across the app.
declare module "@mui/material/styles" {
  interface TypeBackground {
    secondary: string;
  }
}

const shared = {
  shape: {
    borderRadius: 10,
  },
} as const;

const darkTheme = createTheme({
  ...shared,
  palette: {
    mode: "dark",
    primary: { main: "#90DEF1" },
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
  },
});

const lightTheme = createTheme({
  ...shared,
  palette: {
    mode: "light",
    primary: { main: "#2081a3" },
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
  },
});

export { darkTheme, lightTheme };

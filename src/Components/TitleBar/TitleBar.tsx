import CloseRoundedIcon from "@mui/icons-material/CloseRounded";
import RemoveRoundedIcon from "@mui/icons-material/RemoveRounded";
import CropSquareRoundedIcon from "@mui/icons-material/CropSquareRounded";
import DarkModeIcon from "@mui/icons-material/DarkMode";
import LightModeIcon from "@mui/icons-material/LightMode";
import { Paper } from "@mui/material";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./TitleBar.scss";

const appWindow = getCurrentWindow();

interface TitleBarProps {
  isDark: boolean;
  onToggleTheme: () => void;
}

const TitleBar = ({ isDark, onToggleTheme }: TitleBarProps) => {
  return (
    <Paper data-tauri-drag-region id={"title-bar"} elevation={0} square>
      <div className={"title"} data-tauri-drag-region>
        {import.meta.env.VITE_APP_NAME ?? "MXB App by Frost"}
      </div>
      <div className={"win-controls"}>
        <button
          className={"win-btn"}
          onClick={onToggleTheme}
          title={"Toggle theme"}
        >
          {isDark ? <DarkModeIcon /> : <LightModeIcon />}
        </button>
        <button
          className={"win-btn"}
          onClick={() => appWindow.minimize()}
          title={"Minimize"}
        >
          <RemoveRoundedIcon />
        </button>
        <button
          className={"win-btn"}
          onClick={() => appWindow.toggleMaximize()}
          title={"Maximize"}
        >
          <CropSquareRoundedIcon />
        </button>
        <button
          className={"win-btn close"}
          onClick={() => appWindow.close()}
          title={"Close"}
        >
          <CloseRoundedIcon />
        </button>
      </div>
    </Paper>
  );
};

export default TitleBar;

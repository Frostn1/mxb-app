import CloseRoundedIcon from "@mui/icons-material/CloseRounded";
import OpenInFullIcon from "@mui/icons-material/OpenInFull";
import RemoveRoundedIcon from "@mui/icons-material/RemoveRounded";
import DarkModeIcon from "@mui/icons-material/DarkMode";
import LightModeIcon from "@mui/icons-material/LightMode";
import {
  ButtonGroup,
  Divider,
  IconButton,
  Paper,
  useTheme,
} from "@mui/material";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./TitleBar.scss";

const appWindow = getCurrentWindow();

interface TitleBarProps {
  isDark: boolean;
  onToggleTheme: () => void;
}

const TitleBar = ({ isDark, onToggleTheme }: TitleBarProps) => {
  const theme = useTheme();

  return (
    <Paper data-tauri-drag-region id={"title-bar"}>
      <div className={"title"}>
        {import.meta.env.VITE_APP_NAME ?? "MXB App by Frost"}
      </div>
      <ButtonGroup className={"buttons"}>
        <IconButton disableRipple size={"small"} onClick={onToggleTheme}>
          {isDark ? <DarkModeIcon /> : <LightModeIcon />}
        </IconButton>
        <Divider orientation={"vertical"} />
        <div
          onClick={() => appWindow.toggleMaximize()}
          className={"traffic-light-icon"}
          style={{ background: theme.palette.success.main }}
        >
          <OpenInFullIcon className={"inner-icon"} />
        </div>
        <div
          onClick={() => appWindow.minimize()}
          className={"traffic-light-icon"}
          style={{ background: theme.palette.warning.main }}
        >
          <RemoveRoundedIcon className={"inner-icon"} />
        </div>
        <div
          onClick={() => appWindow.close()}
          className={"traffic-light-icon"}
          style={{ background: theme.palette.error.main }}
        >
          <CloseRoundedIcon className={"inner-icon"} />
        </div>
      </ButtonGroup>
    </Paper>
  );
};

export default TitleBar;

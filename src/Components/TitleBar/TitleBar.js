import CloseRoundedIcon from "@mui/icons-material/CloseRounded";
import OpenInFullIcon from "@mui/icons-material/OpenInFull";
import RemoveRoundedIcon from "@mui/icons-material/RemoveRounded";
import {
  ButtonGroup,
  Divider,
  IconButton,
  Paper,
  useTheme,
} from "@mui/material";
import { appWindow } from "@tauri-apps/api/window";
import PropTypes from "prop-types";
import "./TitleBar.scss";
import DarkModeIcon from "@mui/icons-material/DarkMode";
import LightModeIcon from "@mui/icons-material/LightMode";
const DARK = "dark";

const TitleBar = (props) => {
  const theme = useTheme();
  const minimizeWindow = async () => {
    await appWindow.minimize();
  };

  const maximizeWindow = async () => {
    await appWindow.toggleMaximize();
  };

  const closeWindow = async () => {
    await appWindow.close();
  };

  return (
    <Paper data-tauri-drag-region id={"title-bar"}>
      <div className={"title"}>
        {/* <img src={"/icon.ico"} /> */}
        {import.meta.env.VITE_APP_NAME}
      </div>
      <ButtonGroup className={"buttons"}>
        <IconButton
          disableRipple
          size={"small"}
          onClick={() => props.handleChangeTheme(theme.palette.mode !== DARK)}
        >
          {theme.palette.mode === DARK ? <DarkModeIcon /> : <LightModeIcon />}
        </IconButton>
        <Divider orientation={"vertical"} />
        <div
          onClick={maximizeWindow}
          className={"traffic-light-icon"}
          style={{ background: theme.palette.success.main }}
        >
          <OpenInFullIcon className={"inner-icon"} />
        </div>
        <div
          onClick={minimizeWindow}
          className={"traffic-light-icon"}
          style={{ background: theme.palette.warning.main }}
        >
          <RemoveRoundedIcon className={"inner-icon"} />
        </div>
        <div
          onClick={closeWindow}
          className={"traffic-light-icon"}
          style={{ background: theme.palette.error.main }}
        >
          <CloseRoundedIcon className={"inner-icon"} />
        </div>
      </ButtonGroup>
    </Paper>
  );
};

TitleBar.propTypes = {
  handleChangeTheme: PropTypes.func.isRequired,
};

export default TitleBar;

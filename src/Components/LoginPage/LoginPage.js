import ArrowForwardIosRoundedIcon from "@mui/icons-material/ArrowForwardIosRounded";
import {
  Button,
  ButtonGroup,
  Input,
  Paper,
  Slide,
  Slider,
  TextField,
  Tooltip,
  Typography,
  useTheme,
} from "@mui/material";
import PropTypes from "prop-types";
import { useRef, useState } from "react";
import { TransitionGroup } from "react-transition-group";
import "./LoginPage.scss";
import FolderRoundedIcon from "@mui/icons-material/FolderRounded";
import ArrowBackIosNewRoundedIcon from "@mui/icons-material/ArrowBackIosNewRounded";
import { invoke } from "@tauri-apps/api";

const MX_BIKES_POSTFIX = "MX Bikes";

const Custom = (props) => {
  const [modsPath, setModsPath] = useState("");
  const theme = useTheme();

  return (
    <div id={"custom-setup"}>
      <div className={"title"}>Custom Install</div>
      <Paper
        className={"input-container"}
        style={{ background: theme.palette.background.secondary }}
      >
        <Tooltip title={"Example: C:\\User\\Documents\\Piboso\\Mx Bikes"}>
          <TextField
            onChange={(e) => setModsPath(e.target.value)}
            label={"Mod Path "}
            endIcon={<FolderRoundedIcon />}
          />
        </Tooltip>
      </Paper>
      <ButtonGroup
        size={"small"}
        sx={{ gap: 20, height: 100 }}
        color={"primary"}
        className={"actions"}
      >
        <Button
          startIcon={<ArrowBackIosNewRoundedIcon />}
          onClick={props.back}
          className={"button"}
          variant={"contained"}
        >
          Back
        </Button>
        <Button
          onClick={() => props.next({ modsPath })}
          className={"button"}
          variant={"outlined"}
          disabled={!modsPath && modsPath.endsWith(MX_BIKES_POSTFIX)}
        >
          Finish
        </Button>
      </ButtonGroup>
    </div>
  );
};

Custom.propTypes = {
  back: PropTypes.func.isRequired,
  next: PropTypes.func.isRequired,
};

const Default = (props) => {
  const theme = useTheme();
  return (
    <div id={"default"}>
      <div className={"title"}>The MXB App</div>
      <div
        className={"description"}
        style={{ color: theme.palette.text.secondary }}
      >
        This app was designed to help new and veteran players with the weird
        complexicities of mx bikes.
      </div>
      <div
        className={"description"}
        style={{ color: theme.palette.text.secondary }}
      >
        To get started choose between custom or recommanded install.
      </div>
      <ButtonGroup
        size={"small"}
        sx={{ gap: 10, height: 100 }}
        color={"primary"}
        className={"actions"}
      >
        <Button
          onClick={() => props.handleRecommended({ modsPath: "" })}
          className={"button"}
          variant={"contained"}
        >
          Recommended
        </Button>
        <Button
          onClick={() => props.handleCustom()}
          className={"button"}
          variant={"outlined"}
          endIcon={<ArrowForwardIosRoundedIcon />}
        >
          Custom
        </Button>
      </ButtonGroup>
    </div>
  );
};

Default.propTypes = {
  handleRecommended: PropTypes.func.isRequired,
  handleCustom: PropTypes.func.isRequired,
};

const LoginPage = (props) => {
  const theme = useTheme();
  const [isCustomConfig, setIsCustomConfig] = useState(false);
  const containerRef = useRef();
  function configureSettings(config) {
    config = JSON.stringify(config);
    invoke("create_config", { config });
    props.login();
  }
  return (
    <div id={"login-page"}>
      <div
        className={"background"}
        style={{
          backgroundImage: `radial-gradient(${theme.palette.text.disabled} 1px, transparent 1px)`,
        }}
      />
      <div
        className={"author"}
        style={{
          background: `linear-gradient(${theme.palette.text.primary} 0 0) right / 3px 50% no-repeat`,
        }}
      >
        MXBMM
      </div>
      <Paper ref={containerRef} className={"login-section"}>
        <div style={{ overflow: "hidden" }}>
          <Slide
            container={containerRef.current}
            in={!isCustomConfig}
            mountOnEnter
            unmountOnExit
            direction={"right"}
          >
            <div style={{ height: "100%" }}>
              <Default
                handleCustom={() => setIsCustomConfig(true)}
                handleRecommended={configureSettings}
              />
            </div>
          </Slide>
          <Slide
            container={containerRef.current}
            in={isCustomConfig}
            mountOnEnter
            unmountOnExit
            direction={"left"}
          >
            <div style={{ height: "100%" }}>
              <Custom
                back={() => setIsCustomConfig(false)}
                next={configureSettings}
              />
            </div>
          </Slide>
        </div>
      </Paper>
    </div>
  );
};

LoginPage.propTypes = {
  handleRecommended: PropTypes.func.isRequired,
  login: PropTypes.func.isRequired,
};

export default LoginPage;

import ArrowForwardIosRoundedIcon from "@mui/icons-material/ArrowForwardIosRounded";
import ArrowBackIosNewRoundedIcon from "@mui/icons-material/ArrowBackIosNewRounded";
import FolderRoundedIcon from "@mui/icons-material/FolderRounded";
import {
  Button,
  ButtonGroup,
  Paper,
  Slide,
  Typography,
  useTheme,
} from "@mui/material";
import { useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { createConfig } from "../../api/mods";
import type { Config } from "../../types";
import "./LoginPage.scss";

interface StepProps {
  next: (config: Config) => void;
}

const Custom = ({ next, back }: StepProps & { back: () => void }) => {
  const [modsPath, setModsPath] = useState("");
  const theme = useTheme();

  const pickFolder = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select your MX Bikes folder",
    });
    if (typeof selected === "string") setModsPath(selected);
  };

  return (
    <div id={"custom-setup"}>
      <div className={"title"}>Custom Install</div>
      <Paper
        className={"input-container"}
        style={{ background: theme.palette.background.secondary }}
      >
        <Button
          onClick={pickFolder}
          startIcon={<FolderRoundedIcon />}
          variant={"text"}
        >
          {modsPath || "Choose your MX Bikes folder…"}
        </Button>
      </Paper>
      <ButtonGroup
        size={"small"}
        sx={{ gap: 2.5, height: 100 }}
        color={"primary"}
        className={"actions"}
      >
        <Button
          startIcon={<ArrowBackIosNewRoundedIcon />}
          onClick={back}
          className={"button"}
          variant={"contained"}
        >
          Back
        </Button>
        <Button
          onClick={() => next({ modsPath })}
          className={"button"}
          variant={"outlined"}
          disabled={!modsPath}
        >
          Finish
        </Button>
      </ButtonGroup>
    </div>
  );
};

const Default = ({
  next,
  onCustom,
}: StepProps & { onCustom: () => void }) => {
  const theme = useTheme();
  return (
    <div id={"default"}>
      <div className={"title"}>Frost</div>
      <div
        className={"description"}
        style={{ color: theme.palette.text.secondary }}
      >
        Frost helps new and veteran players skip the hassle of installing MX
        Bikes mods — search, click, and it&apos;s in your game.
      </div>
      <div
        className={"description"}
        style={{ color: theme.palette.text.secondary }}
      >
        To get started, choose a recommended or custom install.
      </div>
      <ButtonGroup
        size={"small"}
        sx={{ gap: 1.25, height: 100 }}
        color={"primary"}
        className={"actions"}
      >
        <Button
          onClick={() => next({ modsPath: "" })}
          className={"button"}
          variant={"contained"}
        >
          Recommended
        </Button>
        <Button
          onClick={onCustom}
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

interface LoginPageProps {
  onComplete: () => void;
}

const LoginPage = ({ onComplete }: LoginPageProps) => {
  const theme = useTheme();
  const [isCustom, setIsCustom] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const finish = async (config: Config) => {
    await createConfig(config);
    onComplete();
  };

  return (
    <div id={"login-page"}>
      <div
        className={"background"}
        style={{
          backgroundImage: `radial-gradient(${theme.palette.text.disabled} 1px, transparent 1px)`,
        }}
      />
      <Typography
        className={"author"}
        style={{
          background: `linear-gradient(${theme.palette.text.primary} 0 0) right / 3px 50% no-repeat`,
        }}
      >
        Frost
      </Typography>
      <Paper ref={containerRef} className={"login-section"}>
        <div style={{ overflow: "hidden", height: "100%" }}>
          <Slide
            container={containerRef.current}
            in={!isCustom}
            mountOnEnter
            unmountOnExit
            direction={"right"}
          >
            <div style={{ height: "100%" }}>
              <Default next={finish} onCustom={() => setIsCustom(true)} />
            </div>
          </Slide>
          <Slide
            container={containerRef.current}
            in={isCustom}
            mountOnEnter
            unmountOnExit
            direction={"left"}
          >
            <div style={{ height: "100%" }}>
              <Custom next={finish} back={() => setIsCustom(false)} />
            </div>
          </Slide>
        </div>
      </Paper>
    </div>
  );
};

export default LoginPage;

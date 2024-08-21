import { Button, Paper } from "@mui/material";
import { invoke } from "@tauri-apps/api";
import { useContext, useEffect } from "react";
import path from "path-browserify";
import { ConfigContext } from "../../Context/Config";
import { useState } from "react";
const TRACK_MODS_PATH = "mods/tracks";

const Library = (props) => {
  const Config = useContext(ConfigContext);
  const [mods, setMods] = useState([]);
  const getLibraryMods = async () => {
    setMods(
      await invoke("get_library_mods", {
        libraryPath: path.join(Config.modsPath, TRACK_MODS_PATH),
      }),
    );
  };

  useEffect(() => {
    getLibraryMods();
  }, []);

  return (
    <Paper>
      Library
      <br />
      <pre>{JSON.stringify(mods, null, 2)}</pre>
      <Button onClick={getLibraryMods}>getLibraryMods</Button>
    </Paper>
  );
};

export default Library;

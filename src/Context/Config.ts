import { createContext } from "react";
import type { Config } from "../types";

export const ConfigContext = createContext<Config>({ modsPath: "" });

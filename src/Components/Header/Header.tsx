import {
  Paper,
  Tab,
  Tabs,
  ToggleButton,
  ToggleButtonGroup,
} from "@mui/material";
import TravelExploreRoundedIcon from "@mui/icons-material/TravelExploreRounded";
import LibraryAddCheckRoundedIcon from "@mui/icons-material/LibraryAddCheckRounded";
import { MOD_TYPES, type ModType } from "../../api/mods";
import "./Header.scss";

export type DashboardView = "browse" | "library";

interface HeaderProps {
  view: DashboardView;
  onNavigate: (view: DashboardView) => void;
  modType: ModType;
  onChangeType: (type: ModType) => void;
}

const Header = ({ view, onNavigate, modType, onChangeType }: HeaderProps) => {
  return (
    <Paper id={"header"} elevation={0}>
      <div className={"brand"}>Frost</div>
      <Tabs
        value={view}
        onChange={(_e, v: DashboardView) => onNavigate(v)}
        textColor={"primary"}
        indicatorColor={"primary"}
      >
        <Tab
          value={"browse"}
          icon={<TravelExploreRoundedIcon />}
          iconPosition={"start"}
          label={"Browse"}
        />
        <Tab
          value={"library"}
          icon={<LibraryAddCheckRoundedIcon />}
          iconPosition={"start"}
          label={"Library"}
        />
      </Tabs>
      <ToggleButtonGroup
        className={"type-toggle"}
        size={"small"}
        exclusive
        value={modType.id}
        onChange={(_e, id: string | null) => {
          const next = MOD_TYPES.find((t) => t.id === id);
          if (next) onChangeType(next);
        }}
      >
        {MOD_TYPES.map((t) => (
          <ToggleButton key={t.id} value={t.id}>
            {t.label}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
    </Paper>
  );
};

export default Header;

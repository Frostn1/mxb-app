import {
  Card,
  CardActionArea,
  CardContent,
  CardMedia,
  Chip,
  Typography,
} from "@mui/material";
import TerrainRoundedIcon from "@mui/icons-material/TerrainRounded";
import CheckCircleRoundedIcon from "@mui/icons-material/CheckCircleRounded";
import type { ModSummary } from "../../types";

interface ModCardProps {
  mod: ModSummary;
  installed: boolean;
  onClick: () => void;
}

const ModCard = ({ mod, installed, onClick }: ModCardProps) => {
  return (
    <Card className={"mod-card"}>
      <CardActionArea onClick={onClick}>
        <div className={"media"}>
          {mod.image ? (
            <CardMedia
              component={"img"}
              height={140}
              image={mod.image}
              alt={mod.title}
            />
          ) : (
            <div className={"no-image"}>
              <TerrainRoundedIcon fontSize={"large"} />
            </div>
          )}
          {installed && (
            <Chip
              className={"installed-badge"}
              size={"small"}
              color={"success"}
              icon={<CheckCircleRoundedIcon />}
              label={"In library"}
            />
          )}
        </div>
        <CardContent className={"body"}>
          <Typography variant={"subtitle1"} noWrap title={mod.title}>
            {mod.title}
          </Typography>
          <Typography variant={"caption"} color={"text.secondary"}>
            {new Date(mod.date).toLocaleDateString()}
          </Typography>
        </CardContent>
      </CardActionArea>
    </Card>
  );
};

export default ModCard;

import AppBar from "@frost/shared/Components/Shell/AppBar";
import { useT } from "@/i18n";

/**
 * The chrome for screens that have nothing to navigate: startup, and the setup screen a
 * first run lands on.
 *
 * It exists so a stalled startup can still be dragged and closed — the full rail needs a
 * config before it can render a single nav item, and a window with no controls at all is
 * a window you have to kill from the task manager.
 */
export default function MiniRail() {
  const t = useT();
  return (
    <AppBar
      name={"MXB App"}
      windowLabels={{
        minimize: t("window.minimize"),
        maximize: t("window.maximize"),
        close: t("window.close"),
      }}
    />
  );
}

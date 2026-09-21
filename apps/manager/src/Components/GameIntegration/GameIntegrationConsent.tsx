import { useState } from "react";
import { Loader2, ShieldCheck } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@frost/shared/Components/ui/dialog";
import { useFrostmod } from "../../Context/FrostmodContext";
import { useT } from "@/i18n";

/** Explicit consent for the optional executable that integrates with the running game. */
export default function GameIntegrationConsent({ paused = false }: { paused?: boolean }) {
  const t = useT();
  const {
    integrationConsentOpen,
    status,
    statusError,
    installing,
    enableIntegration,
    useAppOnly: chooseAppOnly,
  } = useFrostmod();
  const [enabling, setEnabling] = useState(false);

  // Existing installs migrate to enabled. Waiting for status avoids flashing the question
  // before that migration has had a chance to run.
  const open = !paused && integrationConsentOpen && (status !== null || statusError);

  const enable = async () => {
    setEnabling(true);
    try {
      await enableIntegration();
    } finally {
      setEnabling(false);
    }
  };

  return (
    <Dialog open={open}>
      <DialogContent
        className="max-w-[500px] gap-5"
        showClose={false}
        onEscapeKeyDown={(event) => event.preventDefault()}
        onPointerDownOutside={(event) => event.preventDefault()}
      >
        <div className="flex items-start gap-3">
          <div className="grid size-10 flex-none place-items-center rounded-xl bg-primary/10 text-primary">
            <ShieldCheck className="size-5" />
          </div>
          <div className="flex min-w-0 flex-col gap-1.5">
            <DialogTitle className="text-[18px] leading-tight">
              {t("setup.integrationTitle")}
            </DialogTitle>
            <DialogDescription>{t("setup.integrationIntro")}</DialogDescription>
          </div>
        </div>

        <div className="rounded-xl border border-input bg-foreground/[0.025] p-3.5 text-[12px] leading-relaxed text-muted-foreground">
          {t("setup.integrationInstallDisclosure")}
        </div>

        <DialogFooter className="justify-between sm:justify-between">
          <Button
            variant="outline"
            onClick={() => void chooseAppOnly()}
            disabled={enabling || installing}
          >
            {t("setup.integrationUseAppOnly")}
          </Button>
          <Button onClick={() => void enable()} disabled={enabling || installing}>
            {(enabling || installing) && <Loader2 className="size-4 animate-spin" />}
            {t("setup.integrationEnable")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

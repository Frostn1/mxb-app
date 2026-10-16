import { useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "../ui/button";
import { useT, type BaseTKey } from "../../i18n/context";
import { supporterCheckout, type CheckoutError } from "../../api/supporter";
import { SUPPORT_URL } from "../../api/support";

const TERMS_URL = "https://mxbsecure.com/terms";

const ERRORS: Record<Exclude<CheckoutError, "notSetUp">, BaseTKey> = {
  steam: "supporter.buyErr.steam",
  owned: "supporter.buyErr.owned",
  failed: "supporter.buyErr.failed",
};

/**
 * Buying the supporter pack: the Terms and the immediate-access waiver, then Stripe Checkout
 * in the browser. Where the pack isn't on sale yet, it goes straight to Buy Me a Coffee.
 * The app picks the pack up on its next start (or Check again where the picker offers it).
 */
export default function SupporterBuy() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [agree, setAgree] = useState(false);
  const [waiver, setWaiver] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BaseTKey | null>(null);

  const coffee = () => void openUrl(SUPPORT_URL).catch(() => {});

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="cursor-default text-left text-[12px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
      >
        {t("supporter.buy")}
      </button>
    );
  }

  const pay = async () => {
    setBusy(true);
    setError(null);
    try {
      await openUrl(await supporterCheckout());
    } catch (e) {
      const why = String(e) as CheckoutError;
      if (why === "notSetUp") coffee();
      else setError(ERRORS[why] ?? ERRORS.failed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <span className="text-[12px] text-muted-foreground">{t("supporter.buyLine")}</span>
      <label className="flex items-start gap-2 text-[12px]">
        <input type="checkbox" checked={agree} onChange={(e) => setAgree(e.target.checked)} className="mt-0.5" />
        <span>
          {t("supporter.agree")}{" "}
          <button
            type="button"
            onClick={() => void openUrl(TERMS_URL).catch(() => {})}
            className="cursor-default underline underline-offset-2"
          >
            {t("supporter.terms")}
          </button>
        </span>
      </label>
      <label className="flex items-start gap-2 text-[12px]">
        <input type="checkbox" checked={waiver} onChange={(e) => setWaiver(e.target.checked)} className="mt-0.5" />
        <span>{t("supporter.waiver")}</span>
      </label>
      <div className="flex flex-wrap items-center gap-1.5">
        <Button size="sm" disabled={busy || !agree || !waiver} onClick={() => void pay()}>
          {t("supporter.pay")}
        </Button>
        <Button size="sm" variant="ghost" onClick={coffee}>
          {t("supporter.orCoffee")}
        </Button>
      </div>
      {error && <span className="text-[12px] text-destructive">{t(error)}</span>}
    </div>
  );
}

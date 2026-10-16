import { useState } from "react";
import { Button } from "../ui/button";
import { useT, type BaseTKey } from "../../i18n/context";
import {
  supporterEmailConfirm,
  supporterEmailSend,
  type EmailError,
} from "../../api/supporter";

const ERRORS: Record<EmailError, BaseTKey> = {
  notSetUp: "supporter.err.notSetUp",
  steam: "supporter.err.steam",
  bad: "supporter.err.bad",
  expired: "supporter.err.expired",
  tooMany: "supporter.err.tooMany",
  failed: "supporter.err.failed",
};

function errorKey(e: unknown): BaseTKey {
  return ERRORS[String(e) as EmailError] ?? ERRORS.failed;
}

const FIELD =
  "h-8 min-w-0 flex-1 rounded-md border border-input bg-transparent px-2.5 text-[12.5px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60";

/**
 * "Bought a coffee? Link the email you used." Two steps: an address, then the code mailed to
 * it. The control plane matches the address against Buy Me a Coffee's payments and grants the
 * pack; nothing here stores the address.
 */
export default function SupporterEmailLink() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [step, setStep] = useState<"email" | "code" | "waiting">("email");
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BaseTKey | null>(null);

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="cursor-default text-left text-[12px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
      >
        {t("supporter.bmacLink")}
      </button>
    );
  }

  const send = async () => {
    setBusy(true);
    setError(null);
    try {
      await supporterEmailSend(email);
      setStep("code");
    } catch (e) {
      setError(errorKey(e));
    } finally {
      setBusy(false);
    }
  };

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      const status = await supporterEmailConfirm(code);
      // With the pack the whole box goes away; without it the coffee hasn't come in yet.
      if (!status.supporter) setStep("waiting");
    } catch (e) {
      setError(errorKey(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <span className="text-[12px] text-muted-foreground">{t("supporter.bmacLink")}</span>
      {step === "email" && (
        <div className="flex gap-1.5">
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder={t("supporter.email")}
            aria-label={t("supporter.email")}
            className={FIELD}
          />
          <Button size="sm" disabled={busy || !email.trim()} onClick={() => void send()}>
            {t("supporter.sendCode")}
          </Button>
        </div>
      )}
      {step === "code" && (
        <>
          <span className="text-[12px] text-muted-foreground">{t("supporter.codeSent")}</span>
          <div className="flex gap-1.5">
            <input
              inputMode="numeric"
              maxLength={6}
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
              placeholder={t("supporter.code")}
              aria-label={t("supporter.code")}
              className={FIELD}
            />
            <Button size="sm" disabled={busy || code.length !== 6} onClick={() => void confirm()}>
              {t("supporter.confirm")}
            </Button>
          </div>
        </>
      )}
      {step === "waiting" && (
        <span className="text-[12px] text-muted-foreground">{t("supporter.linkedNotYet")}</span>
      )}
      {error && <span className="text-[12px] text-destructive">{t(error)}</span>}
    </div>
  );
}

import { ShieldCheck } from "lucide-react";
import { useT } from "../i18n/context";
import type { Trust } from "./manifest";

export type TrustTone = "verified" | "warning" | "danger" | "text";

const DARK = {
  verified: "#32d74b",
  warning: "#ffd60a",
  danger: "#ff6961",
  text: "#f2f2f7",
} as const;

const LIGHT = {
  verified: "#1f7a35",
  warning: "#8a5300",
  danger: "#c4170c",
  text: "#1c1c1e",
} as const;

/** Safety text must stay readable even when an extension replaces the app colour tokens. */
export function trustColor(tone: TrustTone): string {
  const light = typeof document !== "undefined" && document.documentElement.classList.contains("light");
  return (light ? LIGHT : DARK)[tone];
}

export function tierLabelText(
  verified: boolean,
  suggested: boolean,
  labels = { verified: "Verified", suggested: "Suggested", unverified: "Unverified" },
): string {
  if (verified && suggested) return `${labels.verified} · ${labels.suggested}`;
  if (verified) return labels.verified;
  if (suggested) return labels.suggested;
  return labels.unverified;
}

export function TrustLabel({
  verified,
  suggested = false,
  trust,
}: {
  verified?: boolean;
  suggested?: boolean;
  trust?: Trust;
}) {
  const t = useT();
  const isVerified = (verified ?? trust === "verified") || trust === "builtin";
  const unsigned = trust === "unsigned";
  const text = trust === "builtin"
    ? t("ext.trust.builtin")
    : unsigned
      ? t("ext.trust.unsigned")
      : tierLabelText(isVerified, suggested, {
          verified: t("ext.tier.verified"),
          suggested: t("ext.tier.suggested"),
          unverified: t("ext.tier.unverified"),
        });
  const tone: TrustTone = isVerified ? "verified" : unsigned ? "danger" : suggested ? "text" : "warning";

  return (
    <span className="inline-flex items-center gap-1 text-[11.5px]" style={{ color: trustColor(tone) }}>
      {isVerified && <ShieldCheck className="size-3.5 shrink-0" />}
      {text}
    </span>
  );
}

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Whether the signed-in account has the supporter pack (`crates/core/src/supporter.rs`).
 *
 * Cosmetic only: themes, the accent colour, the badge. Never gate anything that exists today
 * on this.
 */
export interface SupporterStatus {
  supporter: boolean;
  /** Unix ms of the control plane's last answer; 0 means never. */
  checkedAt: number;
}

const NONE: SupporterStatus = { supporter: false, checkedAt: 0 };

/** The cached answer. Instant and offline. */
export function supporterStatus(): Promise<SupporterStatus> {
  return invoke<SupporterStatus>("supporter_status");
}

/** Ask the control plane again — after signing in, or after buying the pack. */
export function supporterRefresh(): Promise<SupporterStatus> {
  return invoke<SupporterStatus>("supporter_refresh");
}

/**
 * Why linking a Buy Me a Coffee email failed: `notSetUp`, `steam`, `bad`, `expired`, `tooMany`
 * or `failed`. Each has a `supporter.err.*` string.
 */
export type EmailError = "notSetUp" | "steam" | "bad" | "expired" | "tooMany" | "failed";

/** Mail a code to the address used on Buy Me a Coffee. Rejects with an {@link EmailError}. */
export function supporterEmailSend(email: string): Promise<void> {
  return invoke("supporter_email_send", { email });
}

/** Type the code back; resolves to the fresh status. Rejects with an {@link EmailError}. */
export async function supporterEmailConfirm(code: string): Promise<SupporterStatus> {
  const next = await invoke<SupporterStatus>("supporter_email_confirm", { code });
  window.dispatchEvent(new CustomEvent<SupporterStatus>(CHANGED, { detail: next }));
  return next;
}

/** Fired after a refresh, so every mounted {@link useSupporter} picks the answer up. */
const CHANGED = "frost:supporter-changed";

/** Refresh and tell every listener. */
export async function recheckSupporter(): Promise<SupporterStatus> {
  const next = await supporterRefresh();
  window.dispatchEvent(new CustomEvent<SupporterStatus>(CHANGED, { detail: next }));
  return next;
}

/** The cached status, kept current. A build without the commands reads as "not a supporter". */
export function useSupporter(): SupporterStatus {
  const [status, setStatus] = useState<SupporterStatus>(NONE);
  useEffect(() => {
    let live = true;
    supporterStatus()
      .then((s) => live && setStatus(s))
      .catch(() => {});
    const onChange = (e: Event) => setStatus((e as CustomEvent<SupporterStatus>).detail);
    window.addEventListener(CHANGED, onChange);
    return () => {
      live = false;
      window.removeEventListener(CHANGED, onChange);
    };
  }, []);
  return status;
}

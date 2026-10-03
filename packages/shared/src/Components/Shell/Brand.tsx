import { open as openUrl } from "@tauri-apps/plugin-shell";

/** Who releases the app. The byline in the title bar goes here. */
const ABOUT_URL = "https://mxbsecure.com/about";

/**
 * The wordmark: the product, then who releases it.
 *
 * mxbsecure is the brand every product ships under, so it appears as a byline rather than
 * as the name — the thing you are looking at is the product. Both halves are the brand's
 * mono; the byline sits on the same baseline so the bar keeps its height.
 */
export default function Brand({ name }: { name: string }) {
  return (
    <div data-tauri-drag-region className="flex select-none items-baseline gap-2">
      <span className="font-cond text-[16px] font-extrabold leading-none tracking-[-0.06em]">
        {name}
      </span>
      {/* The byline is a link, and says so only on hover: a permanent underline in the title
          bar would read as chrome rather than as a credit. */}
      <button
        type="button"
        onClick={() => void openUrl(ABOUT_URL)}
        title={ABOUT_URL}
        className="group cursor-default font-cond text-[11px] font-medium leading-none tracking-[-0.02em] text-faint transition-colors hover:text-muted-foreground"
      >
        by{" "}
        <span className="font-semibold text-muted-foreground decoration-muted-foreground/50 underline-offset-[3px] group-hover:underline">
          mxbsecure
        </span>
      </button>
    </div>
  );
}

import type { ReactNode } from "react";
import { ErrorBoundary } from "@frost/shared/Components/ErrorBoundary";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";

/** The grid's "couldn't load mods" panel, with the reason and a Retry. Never automatic. */
export function ListingError({ message, onRetry }: { message: string; onRetry: () => void }) {
  const t = useT();
  return (
    <div className="mx-auto flex max-w-md flex-col items-center gap-3 py-20 text-center">
      <p className="text-[13px] font-semibold text-destructive">{t("browse.loadFailed")}</p>
      {/* The backend explains blocks in plain words; keep them selectable for bug reports. */}
      <p className="select-text text-[12.5px] leading-relaxed text-muted-foreground">
        {message.replace(/^Error:\s*/, "")}
      </p>
      <Button variant="outline" size="sm" onClick={onRetry}>
        {t("common.retry")}
      </Button>
    </div>
  );
}

/**
 * A listing that throws while rendering shows the same error panel as one that failed to
 * load, in place, instead of taking the whole window down to the root fallback. Retry resets
 * the boundary and reloads. Key it on the listing's filters so a new sort starts clean.
 */
export function ListingBoundary({ children, onRetry }: { children: ReactNode; onRetry: () => void }) {
  return (
    <ErrorBoundary
      label="mod-listing"
      fallback={(error, reset) => (
        <ListingError
          message={error.message || String(error)}
          onRetry={() => {
            reset();
            onRetry();
          }}
        />
      )}
    >
      {children}
    </ErrorBoundary>
  );
}

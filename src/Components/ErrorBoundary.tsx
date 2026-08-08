import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, RotateCcw } from "lucide-react";
import { useT } from "../i18n/context";

interface ErrorBoundaryProps {
  children: ReactNode;
  /** Custom fallback. Receives the error + a reset callback (re-mounts children). */
  fallback?: (error: Error, reset: () => void) => ReactNode;
  /** Compact inline fallback instead of the full-screen one (for panels/viewers). */
  compact?: boolean;
  /** Optional label for logs/telemetry so we know which boundary tripped. */
  label?: string;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/* The fallbacks are function components purely so they can reach `useT()` — the
   boundary itself has to stay a class, since only classes can catch. */

function CompactFallback({ error, reset }: { error: Error; reset: () => void }) {
  const t = useT();
  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center text-sm text-muted-foreground">
      <AlertTriangle className="h-6 w-6 text-amber-500" />
      <div>
        <p className="font-medium text-foreground">{t("error.previewFailed")}</p>
        <p className="mt-1 max-w-xs text-xs opacity-80">{error.message}</p>
      </div>
      <button
        type="button"
        onClick={reset}
        className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-accent"
      >
        <RotateCcw className="h-3.5 w-3.5" /> {t("common.tryAgain")}
      </button>
    </div>
  );
}

function FullFallback({ error, reset }: { error: Error; reset: () => void }) {
  const t = useT();
  return (
    <div className="flex h-screen w-screen flex-col items-center justify-center gap-4 bg-background p-8 text-center">
      <AlertTriangle className="h-10 w-10 text-amber-500" />
      <div className="max-w-md">
        <h1 className="text-lg font-semibold text-foreground">
          {t("error.somethingWentWrong")}
        </h1>
        <p className="mt-2 break-words text-sm text-muted-foreground">
          {error.message || t("error.unexpected")}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={reset}
          className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-2 text-sm font-medium text-foreground transition-colors hover:bg-accent"
        >
          <RotateCcw className="h-4 w-4" /> {t("common.tryAgain")}
        </button>
        <button
          type="button"
          onClick={() => window.location.reload()}
          className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-colors hover:opacity-90"
        >
          {t("error.reloadApp")}
        </button>
      </div>
    </div>
  );
}

/**
 * Catches render/lifecycle exceptions in its subtree so one broken component (a
 * malformed model in the 3D viewer, a bad data shape) shows a recoverable error
 * instead of unmounting the whole React tree — which, in the Tauri webview, is an
 * unrecoverable blank white screen. Wrap the whole app once, and any risky
 * subtree (the WebGL canvas) locally so it fails in place.
 */
export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(
      `[ErrorBoundary${this.props.label ? `:${this.props.label}` : ""}]`,
      error,
      info.componentStack,
    );
  }

  reset = () => this.setState({ error: null });

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    if (this.props.fallback) return this.props.fallback(error, this.reset);

    if (this.props.compact) {
      return <CompactFallback error={error} reset={this.reset} />;
    }

    return <FullFallback error={error} reset={this.reset} />;
  }
}

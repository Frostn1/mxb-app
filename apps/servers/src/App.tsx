import { useCallback, useEffect, useState } from "react";
import { Monitor, Moon, Plus, Server as ServerIcon, Sun } from "lucide-react";
import { useTheme, type ThemeChoice } from "@frost/mxbsecure-ui";
import { errorText, listServers, removeServer, type ServerView } from "@/lib/api";
import { FleetCard } from "@/components/FleetCard";
import { ServerDetail } from "@/components/ServerDetail";
import { ServerForm } from "@/components/ServerForm";
import { Button, ErrorLine, type MenuItem } from "@/components/ui";
import { UploadsIndicator } from "@/components/UploadsIndicator";
import { WindowControls } from "@/components/WindowControls";

type View =
  | { kind: "fleet" }
  | { kind: "server"; id: string }
  | { kind: "form"; id: string | null; focusToken?: boolean };

const themeIcons: Record<ThemeChoice, typeof Sun> = { light: Sun, dark: Moon, system: Monitor };
const nextTheme: Record<ThemeChoice, ThemeChoice> = { system: "light", light: "dark", dark: "system" };

export default function App() {
  const [theme, setTheme] = useTheme();
  const [servers, setServers] = useState<ServerView[]>([]);
  const [view, setView] = useState<View>({ kind: "fleet" });
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setServers(await listServers());
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const selected = view.kind === "server" || view.kind === "form" ? servers.find((s) => s.id === view.id) ?? null : null;
  const ThemeIcon = themeIcons[theme];

  // The ⋯ menu for a server: on its card and on its page. Remove confirms in the menu.
  const menu = (server: ServerView): MenuItem[] => [
    { label: "Edit", onSelect: () => setView({ kind: "form", id: server.id }) },
    {
      label: "Remove",
      danger: true,
      confirm: `Remove ${server.name} from this app? Its saved admin token is deleted from this PC too. The server itself is not touched.`,
      onSelect: () => void remove(server),
    },
  ];

  const remove = async (server: ServerView) => {
    try {
      await removeServer(server.id);
      setView({ kind: "fleet" });
      await reload();
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-background text-foreground">
      <header data-tauri-drag-region className="flex h-[52px] shrink-0 select-none items-center border-b bg-window pl-5">
        {/* "MXB Servers by mxbsecure" on one line, the wordmark MXB App and the Coach use. */}
        <div data-tauri-drag-region className="mr-7 flex items-baseline gap-2 whitespace-nowrap font-cond">
          <span data-tauri-drag-region className="text-sm font-extrabold tracking-[-0.04em] text-foreground">MXB Servers</span>
          <span data-tauri-drag-region className="text-[11px] font-medium text-faint">
            by <span className="font-semibold text-muted-foreground">mxbsecure</span>
          </span>
        </div>
        <nav className="flex h-full min-w-0 items-center gap-1 overflow-hidden" aria-label="Servers">
          <button type="button" onClick={() => setView({ kind: "fleet" })} className={`h-full shrink-0 border-b-2 px-3 text-sm font-medium ${view.kind === "fleet" ? "border-primary text-foreground" : "border-transparent text-muted-foreground hover:text-foreground"}`}>All servers</button>
          {servers.map((s) => (
            <button key={s.id} type="button" title={s.name} onClick={() => setView({ kind: "server", id: s.id })} className={`flex h-full min-w-24 max-w-[32rem] items-center gap-2 border-b-2 px-3 text-sm ${selected?.id === s.id && view.kind === "server" ? "border-primary text-foreground" : "border-transparent text-muted-foreground hover:text-foreground"}`}>
              <ServerIcon className="size-3.5 shrink-0" /><span className="truncate">{s.name}</span>
            </button>
          ))}
        </nav>
        <div data-tauri-drag-region className="min-w-4 flex-1" />
        <UploadsIndicator />
        <Button variant="primary" className="mr-2 shrink-0" onClick={() => setView({ kind: "form", id: null })}><Plus className="size-4" /> Add server</Button>
        <button type="button" onClick={() => setTheme(nextTheme[theme])} aria-label={`Theme: ${theme}`} title={`Theme: ${theme}`} className="grid size-9 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent"><ThemeIcon className="size-4" /></button>
        <div className="ml-2 h-5 border-l" />
        <WindowControls />
      </header>

      <main className="min-h-0 min-w-0 flex-1 overflow-auto px-7 py-6 lg:px-9">
        {error && <ErrorLine text={error} />}
        {view.kind === "fleet" && (
          <div className="flex flex-col gap-6">
            <header className="flex flex-col gap-1">
              <h1 className="font-heading text-2xl font-extrabold tracking-tight">All servers</h1>
              <p className="text-sm text-muted-foreground">Status, sessions, tracks, riders, and updates.</p>
            </header>
            {servers.length === 0 ? (
              <p className="max-w-md text-sm text-muted-foreground">
                No servers yet. Add an MXB Server over SSH, or use Legacy connecting for the official dedicated server.
              </p>
            ) : (
              <div className="grid grid-cols-[repeat(auto-fill,minmax(18rem,1fr))] gap-4">
                {servers.map((s) => (
                  <FleetCard key={s.id} server={s} menu={menu(s)} onOpen={() => setView({ kind: "server", id: s.id })} />
                ))}
              </div>
            )}
          </div>
        )}
        {view.kind === "server" && selected && (
          <ServerDetail
            key={selected.id}
            server={selected}
            menu={menu(selected)}
            onSetUpToken={() => setView({ kind: "form", id: selected.id, focusToken: true })}
          />
        )}
        {view.kind === "form" && (
          <ServerForm
            key={view.id ?? "new"}
            initial={selected}
            focusToken={view.focusToken}
            onCancel={() => setView(selected ? { kind: "server", id: selected.id } : { kind: "fleet" })}
            onSaved={(saved) => {
              void reload();
              setView({ kind: "server", id: saved.id });
            }}
          />
        )}
      </main>
    </div>
  );
}

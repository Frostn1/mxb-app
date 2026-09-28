import { useCallback, useEffect, useState } from "react";
import { Monitor, Moon, Plus, Server as ServerIcon, Sun } from "lucide-react";
import { Logo, useTheme, type ThemeChoice } from "@frost/mxbsecure-ui";
import { errorText, listServers, removeServer, type ServerView } from "@/lib/api";
import { FleetCard } from "@/components/FleetCard";
import { ServerDetail } from "@/components/ServerDetail";
import { ServerForm } from "@/components/ServerForm";
import { Button, ErrorLine, type MenuItem } from "@/components/ui";

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
    <div className="flex h-screen">
      <aside className="flex w-60 shrink-0 flex-col gap-1 border-r bg-muted/50 p-3">
        <div className="flex items-baseline gap-1.5 px-2 py-3">
          <Logo className="text-lg" />
          <span className="text-sm text-muted-foreground">servers</span>
        </div>
        <button
          onClick={() => setView({ kind: "fleet" })}
          className={`rounded-md px-2 py-1.5 text-left text-sm font-medium ${view.kind === "fleet" ? "bg-accent" : "hover:bg-accent"}`}
        >
          All servers
        </button>
        {servers.map((s) => (
          <button
            key={s.id}
            onClick={() => setView({ kind: "server", id: s.id })}
            className={`flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm ${
              selected?.id === s.id && view.kind === "server" ? "bg-accent" : "hover:bg-accent"
            }`}
          >
            <ServerIcon className="size-4 text-muted-foreground" />
            <span className="truncate">{s.name}</span>
          </button>
        ))}
        <div className="mt-auto flex items-center gap-2">
          <Button variant="primary" className="flex-1" onClick={() => setView({ kind: "form", id: null })}>
            <Plus className="size-4" /> Add server
          </Button>
          <Button onClick={() => setTheme(nextTheme[theme])} aria-label={`Theme: ${theme}`} title={`Theme: ${theme}`}>
            <ThemeIcon className="size-4" />
          </Button>
        </div>
      </aside>

      <main className="min-w-0 flex-1 overflow-auto p-8">
        {error && <ErrorLine text={error} />}
        {view.kind === "fleet" && (
          <div className="flex flex-col gap-6">
            <h1 className="font-heading text-2xl font-extrabold tracking-tight">All servers</h1>
            {servers.length === 0 ? (
              <p className="max-w-md text-sm text-muted-foreground">
                No servers yet. Add one with its SSH host, user and key; the app forwards the server&apos;s
                observe port over SSH and shows its status here.
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

import { useEffect, useState } from "react";
import { toast } from "sonner";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Copy, ExternalLink, FolderOpen } from "lucide-react";
import { Button } from "../Components/ui/button";
import { Switch } from "../Components/ui/switch";
import { useT } from "../i18n/context";
import { extInspectFile, extSetDeveloperMode, mcpStatus, setMcpEnabled, type McpStatus } from "./api";
import { InstallDialog } from "./ExtensionsPage";
import type { InspectedPackage } from "./manifest";
import { refreshExtensions, useExtensions } from "./store";
import { PublishSection } from "./PublishSection";

const DOCS = "https://github.com/Frostn1/mxb-app/blob/main/docs/extensions.md";
const AGENT_DOCS = "https://github.com/Frostn1/mxb-app/blob/main/docs/extensions-ai-agents.md";
const TEMPLATE = "https://github.com/Frostn1/mxb-app/tree/main/extensions/template";

/** What an agent is told, verbatim; kept in English because it's for the model, not the UI. */
const AGENT_PROMPT = `You are building an MXB App extension. Read docs/extensions.md in github.com/Frostn1/mxb-app first and follow it exactly.
Use the mxb-app MCP server: extension_validate on the folder after every change, extension_install to load it (I approve it in the app), extension_open to show its tab, extension_logs for its console and errors, extension_pack to ship it.
Rules: plain HTML/CSS/JS files and extension.json only; the tab's only API is window.mxb; no Node, Tauri or imports; no network unless the manifest lists the host; ask for the fewest permissions; handle "permission denied"; use the app's CSS variables; textContent, never innerHTML, for other players' data.
Build: `;

async function copy(text: string, done: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(done);
  } catch {
    // No clipboard access: the text is in the box, selectable.
  }
}

function Block({ text, label }: { text: string; label: string }) {
  const t = useT();
  return (
    <div className="relative">
      <pre className="overflow-x-auto whitespace-pre-wrap rounded-md border border-border bg-foreground/[0.04] p-3 pr-20 font-mono text-[11.5px] leading-relaxed text-foreground/85">
        {text}
      </pre>
      <Button size="sm" variant="ghost" className="absolute right-1.5 top-1.5" onClick={() => void copy(text, t("ext.build.copied"))}>
        <Copy className="size-3.5" />
        {label}
      </Button>
    </div>
  );
}

/**
 * Extensions -> Build: how to make one, the developer switches, and the AI agent bridge.
 * The full reference stays in the repo's docs; this is the page that gets someone started.
 */
export default function BuildPage() {
  const t = useT();
  const s = useExtensions();
  const [mcp, setMcp] = useState<McpStatus | null>(null);
  const [pkg, setPkg] = useState<InspectedPackage | null>(null);

  useEffect(() => {
    void mcpStatus().then(setMcp, () => setMcp(null));
  }, []);

  const loadFolder = async () => {
    const path = await openDialog({ directory: true, multiple: false });
    if (typeof path !== "string") return;
    try {
      setPkg(await extInspectFile(path));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const config = JSON.stringify(
    { mcpServers: { "mxb-app": { command: mcp?.command || "MXB App.exe", args: ["--mcp"] } } },
    null,
    2,
  );

  const step = (n: number, title: string, body: string) => (
    <li className="grid grid-cols-[22px_1fr] gap-2">
      <span className="font-cond text-[13px] font-bold text-primary">{n}</span>
      <div>
        <div className="text-[13px] font-semibold text-foreground">{title}</div>
        {/* No ligatures: the body text names commands, and "--key" must not become a dash. */}
        <div className="text-[12.5px] leading-relaxed text-muted-foreground [font-variant-ligatures:none]">{body}</div>
      </div>
    </li>
  );

  return (
    <div className="mx-auto max-w-[860px] space-y-8 pb-6">
      <section className="space-y-4">
        <div>
          <h2 className="text-[18px] font-semibold text-foreground">{t("ext.build.title")}</h2>
          <p className="mt-1 text-[12.5px] text-muted-foreground">{t("ext.build.lede")}</p>
        </div>
        <ol className="space-y-3">
          {step(1, t("ext.build.s1"), t("ext.build.s1b"))}
          {step(2, t("ext.build.s2"), t("ext.build.s2b"))}
          {step(3, t("ext.build.s3"), t("ext.build.s3b"))}
          {step(4, t("ext.build.s4"), t("ext.build.s4b"))}
        </ol>
        <div className="flex flex-wrap items-center gap-2">
          <label className="mr-2 flex items-center gap-2 text-[12.5px] text-foreground/85">
            <Switch checked={s.developerMode} onCheckedChange={(on) => void extSetDeveloperMode(on).then(() => refreshExtensions())} />
            {t("ext.developerMode")}
          </label>
          <Button size="sm" variant="secondary" disabled={!s.developerMode} onClick={() => void loadFolder()}>
            <FolderOpen className="size-3.5" />
            {t("ext.fromFolder")}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => void openUrl(TEMPLATE)}>
            <ExternalLink className="size-3.5" />
            {t("ext.build.template")}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => void openUrl(DOCS)}>
            <ExternalLink className="size-3.5" />
            {t("ext.build.guide")}
          </Button>
        </div>
      </section>

      <PublishSection />

      <section className="space-y-4 border-t border-border pt-6">
        <div>
          <h2 className="text-[18px] font-semibold text-foreground">{t("ext.build.aiTitle")}</h2>
          <p className="mt-1 text-[12.5px] text-muted-foreground">{t("ext.build.aiLede")}</p>
        </div>
        <label className="flex items-center gap-2 text-[12.5px] text-foreground/85">
          <Switch
            checked={Boolean(mcp?.enabled)}
            onCheckedChange={(on) =>
              void setMcpEnabled(on)
                .then(() => mcpStatus())
                .then(setMcp)
                .catch((e) => toast.error(String(e)))
            }
          />
          {t("ext.build.aiSwitch")}
        </label>
        <p className="text-[11.5px] text-muted-foreground">{t("ext.build.aiSafety")}</p>
        <div className="space-y-1.5">
          <div className="text-[12px] font-medium text-foreground/85">{t("ext.build.aiConfig")}</div>
          <Block text={config} label={t("ext.build.copy")} />
        </div>
        <div className="space-y-1.5">
          <div className="text-[12px] font-medium text-foreground/85">{t("ext.build.aiPrompt")}</div>
          <Block text={`${AGENT_PROMPT}<describe your extension here>`} label={t("ext.build.copy")} />
        </div>
        <Button size="sm" variant="ghost" onClick={() => void openUrl(AGENT_DOCS)}>
          <ExternalLink className="size-3.5" />
          {t("ext.build.aiGuide")}
        </Button>
      </section>

      {pkg && <InstallDialog pkg={pkg} onClose={() => setPkg(null)} />}
    </div>
  );
}

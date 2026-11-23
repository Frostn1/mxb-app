import { useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Box, ImagePlus, Loader2, Play, Plus, Sparkles, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import {
  makeAsk,
  makeKeep,
  makePreview,
  makeTemplates,
  type MadePreview,
  type MakeTemplate,
  type Params,
  type Role,
} from "../../../api/bikebuild";

/** Sliders re-render this long after the last change: Blender takes a second or two a picture. */
const SETTLE_MS = 500;

/**
 * The Part Maker: a part from nothing. Describe it, or pick a template and move its sliders;
 * Blender builds it and pictures it; keep it and it's in the tray with its role, snapped to
 * its anchor like any other part.
 */
export default function PartMaker({ ready, onChanged }: { ready: boolean; onChanged: () => void }) {
  const t = useT();
  const [templates, setTemplates] = useState<Record<string, MakeTemplate> | null>(null);
  const [template, setTemplate] = useState("handguards");
  const [params, setParams] = useState<Params>({});
  const [code, setCode] = useState<string | null>(null);
  const [codeRole, setCodeRole] = useState<Role>("handguards");
  const [brief, setBrief] = useState("");
  const [images, setImages] = useState<string[]>([]);
  const [reply, setReply] = useState<{ text: string; by: string } | null>(null);
  const [preview, setPreview] = useState<MadePreview | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState<"ask" | "preview" | "keep" | null>(null);
  /** Only the newest preview is shown; one that finishes late is dropped. */
  const seq = useRef(0);
  const settle = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!ready) return;
    makeTemplates()
      .then(setTemplates)
      .catch((e) => toast.error(t("bike.makerFailed"), { description: String(e) }));
  }, [ready, t]);

  const tpl = templates?.[template];

  // A first picture of the default template, so the panel never opens empty.
  const first = useRef(false);
  useEffect(() => {
    if (templates && !first.current) {
      first.current = true;
      void render({ template, params });
    }
    // Only once, when the templates arrive.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [templates]);

  async function render(req: Parameters<typeof makePreview>[0]) {
    const mine = ++seq.current;
    setBusy("preview");
    try {
      const p = await makePreview(req);
      if (mine === seq.current) setPreview(p);
    } catch (e) {
      if (mine === seq.current) toast.error(t("bike.previewFailed"), { description: String(e) });
    } finally {
      if (mine === seq.current) setBusy(null);
    }
  }

  function renderTemplate(nextTemplate = template, nextParams = params) {
    setCode(null);
    void render({ template: nextTemplate, params: nextParams });
  }

  function setParam(key: string, value: number | string) {
    const next = { ...params, [key]: value };
    setParams(next);
    if (settle.current) clearTimeout(settle.current);
    settle.current = setTimeout(() => renderTemplate(template, next), SETTLE_MS);
  }

  async function onAsk() {
    if (!brief.trim()) return;
    setBusy("ask");
    try {
      const current = code ? { code, role: codeRole } : { template, params };
      const a = await makeAsk(brief, current, images);
      setReply({ text: a.reply, by: a.by });
      if (a.name && !name) setName(a.name);
      if (a.code) {
        // Code is shown before it runs: the rider presses Run.
        setCode(a.code);
        setCodeRole(a.role ?? "handguards");
        setPreview(null);
        setBusy(null);
      } else if (a.template) {
        setTemplate(a.template);
        setParams(a.params);
        setBusy(null);
        renderTemplate(a.template, a.params);
      }
    } catch (e) {
      toast.error(t("bike.askFailed"), { description: String(e) });
      setBusy(null);
    }
  }

  async function onImages() {
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: t("bike.images"), extensions: ["png", "jpg", "jpeg", "webp"] }],
    });
    const files = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    setImages((old) => [...old, ...files].slice(0, 3));
  }

  async function onKeep() {
    setBusy("keep");
    try {
      const part = await makeKeep(name || template);
      toast.success(t("bike.kept", { name: part.name }));
      onChanged();
    } catch (e) {
      toast.error(t("bike.keepFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  const color = typeof params.color === "string" ? params.color : "#ff6600";

  return (
    <section className="flex flex-col gap-3 border border-border bg-card p-4">
      <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("bike.maker")}</h2>
      <p className="text-sm text-muted-foreground">{t("bike.makerHint")}</p>

      <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_20rem]">
        <div className="flex min-w-0 flex-col gap-3">
          <textarea
            className="min-h-20 border border-input bg-transparent p-2 text-sm"
            placeholder={t("bike.briefPlaceholder")}
            value={brief}
            onChange={(e) => setBrief(e.target.value)}
          />
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" onClick={onAsk} disabled={!ready || busy !== null || !brief.trim()}>
              {busy === "ask" ? <Loader2 className="size-3.5 animate-spin" /> : <Sparkles className="size-3.5" />}
              {t("bike.makeIt")}
            </Button>
            <Button size="sm" variant="ghost" onClick={onImages} disabled={images.length >= 3}>
              <ImagePlus className="size-3.5" />
              {t("bike.addImages")}
            </Button>
            {images.map((p) => (
              <span key={p} className="flex items-center gap-1 font-mono text-[11px] text-muted-foreground">
                {p.split(/[\\/]/).pop()}
                <button onClick={() => setImages(images.filter((i) => i !== p))} aria-label={t("bike.removeImage")}>
                  <X className="size-3" />
                </button>
              </span>
            ))}
          </div>
          {reply && (
            <p className="text-[12px] text-muted-foreground">
              {reply.text}{" "}
              <span className="text-faint">
                · {reply.by === "words" ? t("bike.byWords") : t("bike.byModel", { model: reply.by })}
              </span>
            </p>
          )}

          {code ? (
            <div className="flex flex-col gap-2">
              <p className="text-[12px] text-amber-500">{t("bike.codeHint")}</p>
              <pre className="max-h-64 overflow-auto border border-border bg-background p-2 font-mono text-[11px]">
                {code}
              </pre>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => render({ code, role: codeRole })} disabled={busy !== null}>
                  <Play className="size-3.5" />
                  {t("bike.runCode")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => renderTemplate()} disabled={busy !== null}>
                  {t("bike.backToTemplate")}
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <div className="flex flex-wrap items-center gap-2 text-sm">
                <span className="text-muted-foreground">{t("bike.makerTemplate")}</span>
                <select
                  className="h-8 border border-input bg-transparent px-2 text-[12px]"
                  value={template}
                  onChange={(e) => {
                    setTemplate(e.target.value);
                    setParams({});
                    renderTemplate(e.target.value, {});
                  }}
                >
                  {Object.keys(templates ?? {}).map((k) => (
                    <option key={k} value={k}>
                      {k}
                    </option>
                  ))}
                </select>
                {tpl && <span className="text-[12px] text-muted-foreground">{tpl.about}</span>}
              </div>
              {tpl?.sliders.map((s) => {
                const v = typeof params[s.name] === "number" ? (params[s.name] as number) : s.default;
                return (
                  <label key={s.name} className="grid grid-cols-[8rem_1fr_4rem] items-center gap-2 text-[12px]">
                    <span className="text-muted-foreground">{s.name.replace(/_/g, " ")}</span>
                    <input
                      type="range"
                      min={s.min}
                      max={s.max}
                      step={(s.max - s.min) / 100}
                      value={v}
                      onChange={(e) => setParam(s.name, Number(e.target.value))}
                    />
                    <span className="text-right font-mono">{s.max <= 1 ? `${Math.round(v * 1000)} mm` : v.toFixed(0)}</span>
                  </label>
                );
              })}
              {tpl &&
                Object.entries(tpl.choices).map(([k, options]) => (
                  <label key={k} className="grid grid-cols-[8rem_1fr] items-center gap-2 text-[12px]">
                    <span className="text-muted-foreground">{k}</span>
                    <select
                      className="h-8 border border-input bg-transparent px-2"
                      value={(params[k] as string) ?? options[0]}
                      onChange={(e) => setParam(k, e.target.value)}
                    >
                      {options.map((o) => (
                        <option key={o} value={o}>
                          {o}
                        </option>
                      ))}
                    </select>
                  </label>
                ))}
              <label className="grid grid-cols-[8rem_1fr] items-center gap-2 text-[12px]">
                <span className="text-muted-foreground">{t("bike.colour")}</span>
                <input type="color" value={color} onChange={(e) => setParam("color", e.target.value)} />
              </label>
              <div>
                <Button size="sm" variant="outline" onClick={() => renderTemplate()} disabled={!ready || busy !== null}>
                  <Play className="size-3.5" />
                  {t("bike.updatePreview")}
                </Button>
              </div>
            </div>
          )}
        </div>

        <div className="flex flex-col gap-2">
          <div className="relative flex aspect-square w-full items-center justify-center border border-border bg-background">
            {preview?.thumb ? (
              <img src={preview.thumb} alt="" className="size-full object-contain" draggable={false} />
            ) : (
              <Box className="size-8 text-faint" />
            )}
            {busy === "preview" && <Loader2 className="absolute right-2 top-2 size-4 animate-spin text-muted-foreground" />}
          </div>
          {preview && (
            <p className="text-[11px] text-muted-foreground">
              {t("bike.madeStats", {
                role: t(`bike.role.${preview.role}`),
                tris: preview.tris.toLocaleString(),
              })}
              {preview.size && ` · ${preview.size.map((v) => Math.round(v * 1000)).join(" × ")} mm`}
            </p>
          )}
          <input
            className="h-8 border border-input bg-transparent px-2 text-sm"
            placeholder={t("bike.partName")}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Button size="sm" onClick={onKeep} disabled={!preview || busy !== null}>
            {busy === "keep" ? <Loader2 className="size-3.5 animate-spin" /> : <Plus className="size-3.5" />}
            {t("bike.keepPart")}
          </Button>
        </div>
      </div>
    </section>
  );
}

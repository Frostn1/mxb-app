import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { useT } from "@/i18n";
import {
  clearTrackModel,
  getTrackModel,
  setTrackModel,
  testTrackModel,
  type ModelKind,
  type TrackModel,
} from "@/api/trackgen";

interface Provider {
  id: string;
  label?: string;
  kind: ModelKind;
  baseUrl: string;
  model: string;
}

/** Filled in on pick, and all of it editable. Groq's is free and fits Settings only. */
const PROVIDERS: Provider[] = [
  { id: "groq", label: "Groq", kind: "openAi", baseUrl: "https://api.groq.com/openai/v1", model: "openai/gpt-oss-120b" },
  { id: "openrouter", label: "OpenRouter", kind: "openAi", baseUrl: "https://openrouter.ai/api/v1", model: "openai/gpt-oss-120b" },
  { id: "openai", label: "OpenAI", kind: "openAi", baseUrl: "https://api.openai.com/v1", model: "" },
  { id: "ollama", label: "Ollama", kind: "openAi", baseUrl: "http://localhost:11434/v1", model: "llama3.1" },
  { id: "anthropic", label: "Anthropic", kind: "anthropic", baseUrl: "https://api.anthropic.com", model: "claude-opus-5" },
  { id: "custom", kind: "openAi", baseUrl: "", model: "" },
];

function providerOf(m: TrackModel): string {
  const base = m.baseUrl.replace(/\/+$/, "");
  const known = PROVIDERS.find((p) => p.id !== "custom" && p.kind === m.kind && p.baseUrl === base);
  return known?.id ?? (m.kind === "anthropic" ? "anthropic" : "custom");
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="text-[11px] text-muted-foreground">{label}</span>
      <div className="mt-1">{children}</div>
    </label>
  );
}

/**
 * Which model writes tracks: the MXB account, or one of the user's own.
 *
 * The key is typed here once and never shown again. The app keeps it on this computer and
 * reports only whether there is one, so a key left untouched is sent as "keep the saved one".
 */
export default function TrackModelSettings() {
  const t = useT();
  const [provider, setProvider] = useState("account");
  const [kind, setKind] = useState<ModelKind>("openAi");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [key, setKey] = useState("");
  // Switching provider makes the saved key the wrong one, so the field counts as typed.
  const [keyTouched, setKeyTouched] = useState(false);
  const [hasKey, setHasKey] = useState(false);
  const [busy, setBusy] = useState<"test" | "save" | null>(null);

  useEffect(() => {
    getTrackModel()
      .then((m) => {
        if (!m) return;
        setProvider(providerOf(m));
        setKind(m.kind);
        setBaseUrl(m.baseUrl);
        setModel(m.model);
        setHasKey(m.hasKey);
      })
      .catch(() => {});
  }, []);

  function pick(id: string) {
    setProvider(id);
    const p = PROVIDERS.find((x) => x.id === id);
    if (!p) return;
    setKind(p.kind);
    setBaseUrl(p.baseUrl);
    setModel(p.model);
    setKey("");
    setKeyTouched(true);
    setHasKey(false);
  }

  const own = provider !== "account";
  const complete = baseUrl.trim() !== "" && model.trim() !== "";
  const keyArg = keyTouched ? key.trim() : undefined;

  async function onTest() {
    setBusy("test");
    try {
      await testTrackModel(kind, baseUrl, model, keyArg);
      toast.success(t("studioSettings.modelWorks"));
    } catch (e) {
      toast.error(t("studioSettings.modelFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  async function onSave() {
    setBusy("save");
    try {
      if (!own) {
        await clearTrackModel();
        toast.success(t("studioSettings.modelCleared"));
      } else {
        const saved = await setTrackModel(kind, baseUrl, model, keyArg);
        setHasKey(saved.hasKey);
        setKey("");
        setKeyTouched(false);
        toast.success(t("studioSettings.modelSaved"));
      }
    } catch (e) {
      toast.error(t("studioSettings.modelFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="mt-10">
      <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
        {t("studioSettings.model")}
      </div>
      <p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">
        {t("studioSettings.modelHint")}
      </p>
      <select
        value={provider}
        onChange={(e) => pick(e.target.value)}
        className="mt-3 w-full cursor-default border border-border bg-transparent px-3 py-2 text-[13px]"
      >
        <option value="account" className="bg-background">
          {t("studioSettings.modelAccount")}
        </option>
        {PROVIDERS.map((p) => (
          <option key={p.id} value={p.id} className="bg-background">
            {p.label ?? t("studioSettings.modelCustom")}
          </option>
        ))}
      </select>
      {own && (
        <div className="mt-3 grid gap-2">
          <Field label={t("studioSettings.modelAddress")}>
            <Input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} spellCheck={false} />
          </Field>
          <Field label={t("studioSettings.modelName")}>
            <Input value={model} onChange={(e) => setModel(e.target.value)} spellCheck={false} />
          </Field>
          <Field label={t("studioSettings.modelKey")}>
            <Input
              type="password"
              value={key}
              onChange={(e) => {
                setKey(e.target.value);
                setKeyTouched(true);
              }}
              placeholder={
                hasKey && !keyTouched ? t("studioSettings.modelKeySaved") : t("studioSettings.modelKeyNone")
              }
              autoComplete="off"
              spellCheck={false}
            />
          </Field>
        </div>
      )}
      <div className="mt-3 flex items-center gap-2">
        {own && (
          <Button size="sm" variant="outline" onClick={() => void onTest()} disabled={busy !== null || !complete}>
            {busy === "test" ? t("studioSettings.modelTesting") : t("studioSettings.modelTest")}
          </Button>
        )}
        <Button size="sm" onClick={() => void onSave()} disabled={busy !== null || (own && !complete)}>
          {t("studioSettings.modelSave")}
        </Button>
      </div>
    </div>
  );
}

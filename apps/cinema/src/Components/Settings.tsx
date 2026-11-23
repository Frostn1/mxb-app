import { useEffect, useState } from "react";
import { FolderOpen, RotateCcw } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { useTheme, type ThemeMode } from "@frost/shared/Context/Theme";
import { useI18n } from "@frost/shared/i18n/context";
import { LOCALE_OPTIONS, type LocalePref } from "@frost/shared/i18n/core";
import { checkBlender, getSettings, setSettings, type SettingsView } from "@/api/cinema";
import { useT } from "@/i18n";

function Section({ title, desc, children }: { title: string; desc?: string; children: React.ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-3 rounded-xl bg-card p-[18px]">
      <span className="text-[14px] font-bold">{title}</span>
      {desc && <span className="-mt-1.5 text-[12px] text-muted-foreground">{desc}</span>}
      {children}
    </div>
  );
}

function Field({ label, desc, children }: { label: string; desc?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-[12.5px] text-foreground/85">{label}</span>
        {desc && <span className="break-all text-[11.5px] leading-relaxed text-muted-foreground">{desc}</span>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

/** Which Blender, where shots go, and how the app looks and reads. */
export default function Settings() {
  const t = useT();
  const { theme, setTheme } = useTheme();
  const { locale, setLocale } = useI18n();
  const [view, setView] = useState<SettingsView | null>(null);

  useEffect(() => {
    getSettings().then(setView).catch(() => {});
  }, []);

  const save = async (patch: Partial<SettingsView["settings"]>) => {
    if (!view) return;
    try {
      setView(await setSettings({ ...view.settings, ...patch }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const pickBlender = async () => {
    const f = await open({ filters: [{ name: "Blender", extensions: ["exe"] }] });
    if (typeof f !== "string") return;
    const found = await checkBlender(f);
    if (!found) return toast.error(t("settings.notBlender"));
    if (found.tooOld) return toast.error(t("settings.blenderTooOld", { v: found.version }));
    await save({ blenderPath: f });
  };

  const pickFolder = async () => {
    const d = await open({ directory: true });
    if (typeof d === "string") await save({ outputDir: d });
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-[820px] flex-col gap-[18px] px-8 py-8">
        <h2 className="headline text-[24px]">{t("settings.title")}</h2>

        {view && !view.using && (
          <Section title={t("settings.blender")}>
            <Field label={t("settings.blenderNone")}>
              <Button size="sm" onClick={() => void openUrl("https://www.blender.org/download/")}>
                {t("settings.getBlender")}
              </Button>
              <Button size="sm" variant="outline" onClick={() => void pickBlender()}>
                {t("settings.choose")}
              </Button>
            </Field>
          </Section>
        )}

        <Section title={t("settings.output")}>
          <Field label={t("settings.outputFolder")} desc={view?.output}>
            {view?.settings.outputDir && (
              <Button size="sm" variant="ghost" onClick={() => void save({ outputDir: "" })} title={t("settings.auto")}>
                <RotateCcw className="size-3.5" />
              </Button>
            )}
            <Button size="sm" variant="outline" onClick={() => void pickFolder()}>
              <FolderOpen className="size-3.5" />
              {t("settings.choose")}
            </Button>
          </Field>
        </Section>

        <Section title={t("settings.appearance")}>
          <Field label={t("settings.theme")}>
            <Segmented
              size="sm"
              value={theme}
              onChange={(v) => setTheme(v as ThemeMode)}
              options={[
                { value: "light", label: t("settings.themeLight") },
                { value: "dark", label: t("settings.themeDark") },
                { value: "system", label: t("settings.themeSystem") },
              ]}
            />
          </Field>
          <Field label={t("settings.language")}>
            <select
              value={locale}
              onChange={(e) => setLocale(e.target.value as LocalePref)}
              className="h-8 rounded-lg border border-input bg-card px-2 text-[12.5px]"
            >
              {LOCALE_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.value === "system" ? t("settings.themeSystem") : o.label}
                </option>
              ))}
            </select>
          </Field>
        </Section>
      </div>
    </div>
  );
}

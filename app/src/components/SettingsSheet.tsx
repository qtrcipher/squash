import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type Settings } from "../api";
import { applyTheme } from "../format";
import i18n from "../i18n";
import Sheet from "./Sheet";

/**
 * S6 settings (docs/03 §2 S6): controls reflect stored values; a failed
 * persist shows a non-blocking banner with Retry — the screen never blocks.
 * Language flips `dir="rtl"` live (via i18n's languageChanged hook);
 * theme applies `data-theme`.
 */
export default function SettingsSheet({
  settings,
  readOnly,
  onSaved,
  onClose,
}: {
  settings: Settings;
  readOnly: boolean;
  onSaved: (settings: Settings) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(settings);
  const [saveError, setSaveError] = useState(false);

  const update = (patch: Partial<Settings>) => {
    const next = { ...draft, ...patch };
    setDraft(next);
    setSaveError(false);
    // Live side effects first — the UI responds even if the persist fails.
    if (patch.language) void i18n.changeLanguage(patch.language);
    if (patch.theme) applyTheme(patch.theme);
    void persist(next);
  };

  const persist = async (next: Settings) => {
    try {
      await api.setSettings(next);
      onSaved(next);
    } catch {
      setSaveError(true);
    }
  };

  return (
    <Sheet title={t("settings.title")} onClose={onClose}>
      {readOnly && (
        <p className="banner warning" role="status">
          {t("settings.readOnlyWarning")}
        </p>
      )}
      {saveError && (
        <p className="banner error" role="alert">
          {t("settings.saveFailed")}{" "}
          <button type="button" className="button small" onClick={() => void persist(draft)}>
            {t("actions.retry")}
          </button>
        </p>
      )}

      <div className="field">
        <label className="field-label" htmlFor="settings-language">
          {t("settings.language")}
        </label>
        <select
          id="settings-language"
          className="input"
          value={draft.language}
          data-autofocus
          onChange={(event) => update({ language: event.target.value as Settings["language"] })}
        >
          <option value="en">English</option>
          <option value="ar">العربية</option>
        </select>
      </div>

      <div className="field">
        <label className="field-label" htmlFor="settings-theme">
          {t("settings.theme")}
        </label>
        <select
          id="settings-theme"
          className="input"
          value={draft.theme}
          onChange={(event) => update({ theme: event.target.value as Settings["theme"] })}
        >
          <option value="system">{t("settings.themes.system")}</option>
          <option value="light">{t("settings.themes.light")}</option>
          <option value="dark">{t("settings.themes.dark")}</option>
        </select>
      </div>

      <div className="field">
        <label className="field-label" htmlFor="settings-preset">
          {t("settings.defaultPreset")}
        </label>
        <select
          id="settings-preset"
          className="input"
          value={draft.default_preset}
          onChange={(event) => update({ default_preset: event.target.value })}
        >
          <option value="builtin:fast">{t("compress.presets.fast")}</option>
          <option value="builtin:balanced">{t("compress.presets.balanced")}</option>
          <option value="builtin:max">{t("compress.presets.max")}</option>
        </select>
      </div>

      <div className="field">
        <label className="field-label" htmlFor="settings-format">
          {t("settings.defaultFormat")}
        </label>
        <select
          id="settings-format"
          className="input"
          value={draft.default_format}
          onChange={(event) => update({ default_format: event.target.value })}
        >
          <option value="zip">zip</option>
          <option value="7z">7z</option>
          <option value="tar.gz">tar.gz</option>
          <option value="tar.zst">tar.zst</option>
        </select>
      </div>
    </Sheet>
  );
}

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type Settings } from "../api";
import { initCrashReporting, shutdownCrashReporting } from "../crashReporting";
import { applyTheme } from "../format";
import i18n from "../i18n";
import { completeFirstLaunch } from "../state/onboarding";
import CrashReportingField from "./CrashReportingField";
import Sheet from "./Sheet";

/**
 * S7 first-launch sheet (docs/03 §2 S7, F1): pick language + theme, optional
 * "make default handler", crash-reporting consent (unchecked by default,
 * docs/06 §6), Continue. Success-only, skippable, never shown again —
 * closing it any way (Continue, Esc, scrim) marks first launch done.
 *
 * The default-handler step is honest (docs/03 F6): the OS owns file
 * associations, so instead of a fake checkbox the sheet offers to open the
 * OS default-apps UI and falls back to manual instructions where no such
 * panel exists.
 */
export default function WelcomeSheet({
  settings,
  readOnly,
  crashReportingAvailable,
  onSaved,
  onDone,
}: {
  settings: Settings;
  readOnly: boolean;
  crashReportingAvailable: boolean;
  onSaved: (settings: Settings) => void;
  onDone: (settings: Settings) => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(settings);
  const [saveError, setSaveError] = useState(false);
  const [showManualSteps, setShowManualSteps] = useState(false);

  const update = (patch: Partial<Settings>) => {
    const next = { ...draft, ...patch };
    setDraft(next);
    setSaveError(false);
    // Live side effects first — the UI responds even if the persist fails.
    if (patch.language) void i18n.changeLanguage(patch.language);
    if (patch.theme) applyTheme(patch.theme);
    // Crash reporting (docs/06 §6): on → init the SDK now; off → close it.
    if (patch.crash_reporting === true) {
      void api
        .crashReportingConfig()
        .then((config) => initCrashReporting({ consent: true, config, locale: next.language }))
        .catch(() => undefined);
    } else if (patch.crash_reporting === false) {
      void shutdownCrashReporting();
    }
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

  const openOsSettings = async () => {
    try {
      await api.openDefaultAppsSettings();
    } catch {
      setShowManualSteps(true);
    }
  };

  /** Continue and Esc/scrim alike: mark done (best-effort persist) and close. */
  const finish = () => {
    const next = completeFirstLaunch(draft);
    onDone(next);
    void api.setSettings(next).catch(() => undefined);
  };

  return (
    <Sheet title={t("onboarding.title")} onClose={finish}>
      <p className="sheet-summary">{t("onboarding.intro")}</p>

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
        <label className="field-label" htmlFor="onboarding-language">
          {t("settings.language")}
        </label>
        <select
          id="onboarding-language"
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
        <label className="field-label" htmlFor="onboarding-theme">
          {t("settings.theme")}
        </label>
        <select
          id="onboarding-theme"
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
        <span className="field-label">{t("onboarding.defaultHandler")}</span>
        <p className="sheet-note">{t("onboarding.defaultHandlerNote")}</p>
        <div>
          <button type="button" className="button" onClick={() => void openOsSettings()}>
            {t("onboarding.openOsSettings")}
          </button>
        </div>
        {showManualSteps && <p className="sheet-note">{t("onboarding.manualSteps")}</p>}
      </div>

      <CrashReportingField
        id="onboarding-crash-reporting"
        checked={draft.crash_reporting}
        available={crashReportingAvailable}
        onChange={(crash_reporting) => update({ crash_reporting })}
      />

      <div className="sheet-actions">
        <button type="button" className="button primary cta" onClick={finish}>
          {t("onboarding.continue")}
        </button>
      </div>
    </Sheet>
  );
}

import { useTranslation } from "react-i18next";

/**
 * The crash-reporting consent control shared by S6 and S7 (docs/06 §6):
 * a checkbox — UNCHECKED by default — with plain-language text stating
 * exactly what a report contains and what it never will. When the build
 * ships no DSN the box is disabled and the note explains it is not
 * available in this build.
 */
export default function CrashReportingField({
  id,
  checked,
  available,
  onChange,
}: {
  id: string;
  checked: boolean;
  available: boolean;
  onChange: (checked: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="field">
      <label className="field-label" htmlFor={id}>
        {t("crashReporting.label")}
      </label>
      <div className="field-row">
        <input
          id={id}
          type="checkbox"
          checked={checked && available}
          disabled={!available}
          onChange={(event) => onChange(event.target.checked)}
        />
      </div>
      <p className="sheet-note">
        {t(available ? "crashReporting.note" : "crashReporting.unavailable")}
      </p>
    </div>
  );
}

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type ArchiveRef, type JobEntry } from "../api";
import { parentDir } from "../format";
import Sheet from "./Sheet";

/**
 * S3 extract sheet (docs/03 §2 S3, flow F3). Destination picker; the
 * single-root-vs-loose layout decision is made by the core at extraction
 * time (docs/03 F3), surfaced here as an informational note.
 */
export default function ExtractSheet({
  archives,
  onSubmitted,
  onClose,
}: {
  archives: ArchiveRef[];
  onSubmitted: (entries: JobEntry[]) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [destination, setDestination] = useState(() => parentDir(archives[0]?.path ?? ""));
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const browse = async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") setDestination(selected);
  };

  const submit = async () => {
    setSubmitting(true);
    setSubmitError(null);
    try {
      // F4: each archive becomes its own queue row.
      const entries: JobEntry[] = [];
      for (const archive of archives) {
        entries.push(await api.submitExtract(archive.path, destination, archive.format));
      }
      onSubmitted(entries);
    } catch (error) {
      setSubmitError(t("extract.submitFailed", { message: String(error) }));
      setSubmitting(false);
    }
  };

  return (
    <Sheet title={t("extract.title")} onClose={onClose}>
      <p className="sheet-summary">
        {t("extract.archivesSummary", { count: archives.length })}
      </p>

      <div className="field">
        <label className="field-label" htmlFor="extract-destination">
          {t("extract.destination")}
        </label>
        <div className="field-row">
          <input
            id="extract-destination"
            className="input mono"
            type="text"
            dir="ltr"
            value={destination}
            onChange={(event) => setDestination(event.target.value)}
          />
          <button type="button" className="button" onClick={() => void browse()}>
            {t("actions.browse")}
          </button>
        </div>
      </div>

      <p className="sheet-note">{t("extract.autoFolderNote")}</p>

      {submitError && (
        <p className="field-error" role="alert">
          {submitError}
        </p>
      )}

      <div className="sheet-actions">
        <button type="button" className="button" onClick={onClose}>
          {t("actions.cancel")}
        </button>
        <button
          type="button"
          className="button primary cta"
          disabled={destination.trim() === "" || submitting}
          onClick={() => void submit()}
        >
          {t("actions.extract")}
        </button>
      </div>
    </Sheet>
  );
}

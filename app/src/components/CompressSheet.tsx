import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type ItemRef, type JobEntry } from "../api";
import { baseName, formatBytes, isolate, joinPath, parentDir } from "../format";
import Sheet from "./Sheet";
import SegmentedControl from "./SegmentedControl";

const FORMATS = ["zip", "7z", "tar.gz", "tar.zst"] as const;
const PRESETS = ["fast", "balanced", "max"] as const;

/**
 * S2 compress sheet (docs/03 §2 S2, flows F2/F4). Batch default is one
 * archive per item; a combined archive is the alternative (F4).
 */
export default function CompressSheet({
  items,
  totalBytes,
  defaultFormat,
  defaultPreset,
  onSubmitted,
  onClose,
}: {
  items: ItemRef[];
  totalBytes: number | null;
  defaultFormat: string;
  defaultPreset: string;
  onSubmitted: (entries: JobEntry[]) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [format, setFormat] = useState<string>(
    (FORMATS as readonly string[]).includes(defaultFormat) ? defaultFormat : "zip",
  );
  const [preset, setPreset] = useState<string>(
    (PRESETS as readonly string[]).includes(defaultPreset) ? defaultPreset : "balanced",
  );
  const [name, setName] = useState(() => baseName(items[0]?.path ?? ""));
  const [location, setLocation] = useState(() => parentDir(items[0]?.path ?? ""));
  const [mode, setMode] = useState<"perItem" | "combined">("perItem");
  const [nameError, setNameError] = useState<string | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const batch = items.length > 1;
  const combined = !batch || mode === "combined";

  /** Every output path this submission would write. */
  const outputs = useMemo(() => {
    if (combined) {
      return [joinPath(location, `${name}.${format}`)];
    }
    return items.map((item) =>
      joinPath(parentDir(item.path), `${baseName(item.path)}.${format}`),
    );
  }, [combined, items, location, name, format]);

  // Inline validation (docs/03 S2 error state): empty name or an existing
  // output blocks the primary action with a field-level message.
  useEffect(() => {
    let stale = false;
    if (combined && name.trim() === "") {
      setNameError(t("compress.nameRequired"));
      return;
    }
    void Promise.all(outputs.map((path) => api.pathExists(path))).then((exists) => {
      if (stale) return;
      setNameError(exists.some(Boolean) ? t("compress.nameExists") : null);
    });
    return () => {
      stale = true;
    };
  }, [outputs, combined, name, t]);

  const browse = async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") setLocation(selected);
  };

  const submit = async () => {
    setSubmitting(true);
    setSubmitError(null);
    try {
      const entries: JobEntry[] = [];
      if (combined) {
        entries.push(
          await api.submitCompress(
            items.map((item) => item.path),
            outputs[0],
            format,
            `builtin:${preset}`,
          ),
        );
      } else {
        // F4: each item becomes its own queue row; failures don't block the rest.
        for (let i = 0; i < items.length; i++) {
          entries.push(
            await api.submitCompress([items[i].path], outputs[i], format, `builtin:${preset}`),
          );
        }
      }
      onSubmitted(entries);
    } catch (error) {
      setSubmitError(t("compress.submitFailed", { message: String(error) }));
      setSubmitting(false);
    }
  };

  /** Return activates the primary action from any text field (docs/03 §5). */
  const submitOnEnter = (event: React.KeyboardEvent) => {
    if (event.key === "Enter" && nameError === null && !submitting) {
      event.preventDefault();
      void submit();
    }
  };

  return (
    <Sheet title={t("compress.title")} onClose={onClose}>
      <p className="sheet-summary">
        {t("compress.itemsSummary", { count: items.length })}
        {totalBytes !== null && <> — {t("compress.totalSize", { size: isolate(formatBytes(totalBytes)) })}</>}
      </p>

      <div className="field">
        <span className="field-label" id="compress-format-label">
          {t("compress.format")}
        </span>
        <SegmentedControl
          labelId="compress-format-label"
          options={FORMATS.map((f) => ({ value: f as string, label: f }))}
          value={format}
          onChange={setFormat}
          autoFocus={!combined}
        />
      </div>

      <div className="field">
        <span className="field-label" id="compress-preset-label">
          {t("compress.preset")}
        </span>
        <SegmentedControl
          labelId="compress-preset-label"
          options={PRESETS.map((p) => ({ value: p as string, label: t(`compress.presets.${p}`) }))}
          value={preset}
          onChange={setPreset}
        />
      </div>

      {batch && (
        <div className="field">
          <span className="field-label" id="compress-mode-label">
            {t("compress.batchMode")}
          </span>
          <SegmentedControl
            labelId="compress-mode-label"
            options={[
              { value: "perItem", label: t("compress.onePerItem") },
              { value: "combined", label: t("compress.combined") },
            ]}
            value={mode}
            onChange={(next) => setMode(next as "perItem" | "combined")}
          />
        </div>
      )}

      {combined && (
        <div className="field">
          <label className="field-label" htmlFor="compress-name">
            {t("compress.outputName")}
          </label>
          <input
            id="compress-name"
            className="input"
            type="text"
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={submitOnEnter}
            data-autofocus
            aria-invalid={nameError !== null}
            aria-describedby={nameError ? "compress-name-error" : undefined}
          />
        </div>
      )}

      <div className="field">
        <label className="field-label" htmlFor="compress-location">
          {t("compress.outputLocation")}
        </label>
        <div className="field-row">
          <input
            id="compress-location"
            className="input mono"
            type="text"
            dir="ltr"
            value={location}
            onChange={(event) => setLocation(event.target.value)}
            onKeyDown={submitOnEnter}
          />
          <button type="button" className="button" onClick={() => void browse()}>
            {t("actions.browse")}
          </button>
        </div>
      </div>

      {nameError && (
        <p className="field-error" id="compress-name-error" role="alert">
          {nameError}
        </p>
      )}
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
          disabled={nameError !== null || submitting}
          onClick={() => void submit()}
        >
          {t("actions.compress")}
        </button>
      </div>
    </Sheet>
  );
}

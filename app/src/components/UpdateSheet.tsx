import { useTranslation } from "react-i18next";
import type { UpdateState } from "../state/updater";
import Sheet from "./Sheet";

/**
 * D3 update-available sheet (docs/03 §2 D3): version + notes with
 * "Download and install" (the updater plugin's signed install path) and
 * "Later". Installing shows progress as text states (downloading → restart
 * prompt); a failure shows a non-blocking error with Retry, matching the
 * four-state discipline.
 */
export default function UpdateSheet({
  state,
  onInstall,
  onRestart,
  onLater,
}: {
  state: Extract<UpdateState, { kind: "available" | "downloading" | "readyToRestart" }>;
  onInstall: () => void;
  onRestart: () => void;
  onLater: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Sheet title={t("updates.availableTitle")} onClose={onLater}>
      <p className="sheet-summary">{t("updates.availableBody", { version: state.info.version })}</p>
      {state.info.notes && <p className="sheet-note">{state.info.notes}</p>}

      {state.kind === "downloading" && (
        <p className="sheet-note" role="status">
          {t("updates.downloading")}
        </p>
      )}
      {state.kind === "readyToRestart" && (
        <p className="sheet-note" role="status">
          {t("updates.readyToRestart")}
        </p>
      )}

      <div className="sheet-actions">
        {state.kind === "available" && (
          <>
            <button type="button" className="button" onClick={onLater}>
              {t("updates.later")}
            </button>
            <button
              type="button"
              className="button primary cta"
              data-autofocus
              onClick={onInstall}
            >
              {t("updates.downloadAndInstall")}
            </button>
          </>
        )}
        {state.kind === "readyToRestart" && (
          <>
            <button type="button" className="button" onClick={onLater}>
              {t("updates.later")}
            </button>
            <button type="button" className="button primary cta" data-autofocus onClick={onRestart}>
              {t("updates.restartNow")}
            </button>
          </>
        )}
      </div>
    </Sheet>
  );
}

import { useTranslation } from "react-i18next";

/**
 * S1 — main window / drop zone (docs/03 §1), empty state only.
 * The drop-zone illustration IS the empty state; the job queue section
 * collapses entirely when empty (docs/03 §2 S4), so it is not rendered here.
 * Static shell: no Tauri commands wired yet.
 */
export default function App() {
  const { t } = useTranslation();

  return (
    <div className="app-shell">
      <header className="toolbar">
        <span className="app-name">{t("app.title")}</span>
        <button type="button" className="button" disabled>
          {t("settings.title")}
        </button>
      </header>
      <main className="main-area">
        <div className="drop-zone" role="group" aria-label={t("dropZone.a11yLabel")}>
          <DropIcon />
          <p className="prompt">{t("dropZone.prompt")}</p>
          <span>{t("dropZone.or")}</span>
          <button type="button" className="button primary">
            {t("dropZone.chooseFiles")}
          </button>
        </div>
      </main>
    </div>
  );
}

/** Placeholder drop glyph; final art is a Lucide-based custom set (docs/04 §4). */
function DropIcon() {
  return (
    <svg
      width="48"
      height="48"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M21 8v13H3V8" />
      <path d="M1 3h22v5H1z" />
      <path d="M10 12h4" />
    </svg>
  );
}

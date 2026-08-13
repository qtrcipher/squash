import { useEffect, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

/**
 * Modal sheet shell (docs/03 §1: everything beyond S1 is a sheet attached to
 * the main window). Esc cancels; clicking the scrim cancels; the primary
 * action lives in the sheet body.
 */
export default function Sheet({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="sheet-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="sheet" role="dialog" aria-modal="true" aria-label={title}>
        <header className="sheet-header">
          <h2 className="sheet-title">{title}</h2>
          <button type="button" className="button" onClick={onClose}>
            {t("actions.close")}
          </button>
        </header>
        <div className="sheet-body">{children}</div>
      </div>
    </div>
  );
}

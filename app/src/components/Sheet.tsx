import { useEffect, useId, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { FOCUSABLE_SELECTOR, trapFocusTarget } from "../a11y";

/**
 * Modal sheet shell (docs/03 §1: everything beyond S1 is a sheet attached to
 * the main window). Esc cancels; clicking the scrim cancels; the primary
 * action lives in the sheet body.
 *
 * Accessibility contract (docs/03 §5): focus moves INTO the sheet on open
 * (`[data-autofocus]` target first, else the first focusable control), Tab is
 * trapped while the sheet is modal, focus returns to the trigger on close,
 * and the background is inert + hidden from assistive tech for the duration.
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
  const titleId = useId();
  const overlayRef = useRef<HTMLDivElement>(null);
  // The trap/focus effects install once per mount (a sheet never re-opens
  // without remounting); callers pass fresh closures, so read the latest
  // onClose through a ref instead of re-running — re-running would steal
  // focus back to the autofocus target on every render.
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const overlay = overlayRef.current;
    const sheet = overlay?.querySelector<HTMLElement>(".sheet");
    if (!overlay || !sheet) return;

    // Focus into the sheet on open; restore it to the trigger on close.
    const previouslyFocused =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusables = () =>
      Array.from(sheet.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
        (el) => !el.hasAttribute("disabled"),
      );
    const preferred = sheet.querySelector<HTMLElement>("[data-autofocus]");
    (preferred ?? focusables()[0] ?? sheet).focus();

    // While modal, the rest of the window is inert and hidden from AT. Live
    // regions stay live so job announcements still land (docs/03 §5).
    const hiddenSiblings: HTMLElement[] = [];
    for (const el of Array.from(overlay.parentElement?.children ?? [])) {
      if (el === overlay || !(el instanceof HTMLElement) || el.hasAttribute("aria-live")) {
        continue;
      }
      el.setAttribute("inert", "");
      el.setAttribute("aria-hidden", "true");
      hiddenSiblings.push(el);
    }

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const items = focusables();
      if (items.length === 0) {
        event.preventDefault();
        return;
      }
      const current = items.indexOf(document.activeElement as HTMLElement);
      const target = trapFocusTarget(current, event.shiftKey, items.length);
      if (target !== null) {
        event.preventDefault();
        items[target].focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      for (const el of hiddenSiblings) {
        el.removeAttribute("inert");
        el.removeAttribute("aria-hidden");
      }
      if (previouslyFocused && document.contains(previouslyFocused)) {
        previouslyFocused.focus();
      }
    };
    // Mount-only by design (see the onCloseRef note above).
  }, []);

  return (
    <div
      ref={overlayRef}
      className="sheet-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        <header className="sheet-header">
          <h2 className="sheet-title" id={titleId}>
            {title}
          </h2>
          <button type="button" className="button" onClick={onClose}>
            {t("actions.close")}
          </button>
        </header>
        <div className="sheet-body">{children}</div>
      </div>
    </div>
  );
}

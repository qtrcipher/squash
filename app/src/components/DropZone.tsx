import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

/**
 * S1 drop zone (docs/03 §1). Uses Tauri's native file-drop event — it
 * delivers real filesystem paths, which HTML5 drag-and-drop cannot.
 * The illustration IS the empty state (docs/03 §2 S1).
 */
export default function DropZone({ onPaths }: { onPaths: (paths: string[]) => void }) {
  const { t } = useTranslation();
  const [dragOver, setDragOver] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          setDragOver(true);
        } else if (payload.type === "leave") {
          setDragOver(false);
        } else if (payload.type === "drop") {
          setDragOver(false);
          if (payload.paths.length > 0) onPaths(payload.paths);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, [onPaths]);

  const chooseFiles = async () => {
    const selected = await open({ multiple: true });
    if (selected && selected.length > 0) onPaths(selected);
  };

  return (
    // The whole zone is the SR/keyboard "button equivalent" (docs/03 §5):
    // Enter/Space/click all open the file picker, and the zone carries the
    // role + instructions. The visual "Choose Files…" chip is a styled span,
    // not a nested button — nested interactives would break the control.
    <div
      className={dragOver ? "drop-zone drag-over" : "drop-zone"}
      role="button"
      tabIndex={0}
      aria-label={t("dropZone.a11yLabel")}
      aria-describedby="drop-zone-instructions"
      onClick={() => void chooseFiles()}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault(); // keep Space from scrolling the window
          void chooseFiles();
        }
      }}
    >
      <DropIcon />
      <p className="prompt">{dragOver ? t("dropZone.dragOver") : t("dropZone.prompt")}</p>
      <span id="drop-zone-instructions" className="visually-hidden">
        {t("dropZone.a11yHint")}
      </span>
      <span aria-hidden="true">{t("dropZone.or")}</span>
      <span className="button primary" aria-hidden="true">
        {t("dropZone.chooseFiles")}
      </span>
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

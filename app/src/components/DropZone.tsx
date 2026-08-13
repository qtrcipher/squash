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
    <div
      className={dragOver ? "drop-zone drag-over" : "drop-zone"}
      role="group"
      aria-label={t("dropZone.a11yLabel")}
    >
      <DropIcon />
      <p className="prompt">{dragOver ? t("dropZone.dragOver") : t("dropZone.prompt")}</p>
      <span>{t("dropZone.or")}</span>
      <button type="button" className="button primary" onClick={() => void chooseFiles()}>
        {t("dropZone.chooseFiles")}
      </button>
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

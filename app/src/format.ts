/**
 * Display helpers. Numbers stay in Western Arabic numerals in both locales
 * (docs/03 §6); paths/names are data and always render LTR with bidi
 * isolation, even inside an RTL window.
 */

/** Human byte size, 1024-based (1.2 GB). */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = "B";
  for (const next of units) {
    if (value < 1024) break;
    value /= 1024;
    unit = next;
  }
  const rounded = value >= 100 ? Math.round(value) : Math.round(value * 10) / 10;
  return `${rounded} ${unit}`;
}

/** Last path component, tolerant of both separators and trailing slashes. */
export function baseName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return cut >= 0 ? trimmed.slice(cut + 1) : trimmed;
}

/** Parent directory of a path ("/a/b" → "/a", "C:\\a\\b" → "C:\\a"). */
export function parentDir(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (cut < 0) return trimmed;
  if (cut === 0) return trimmed.slice(0, 1);
  return trimmed.slice(0, cut);
}

/** Join a directory and a file name with the platform's separator style. */
export function joinPath(dir: string, name: string): string {
  if (/[\\/]$/.test(dir)) return dir + name;
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return dir + sep + name;
}

/** Apply the theme setting to the document (`data-theme`, docs/04 §2). */
export function applyTheme(theme: "system" | "light" | "dark"): void {
  if (theme === "system") {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = theme;
  }
}

/**
 * Wrap a data value (file name, byte size) in LTR bidi isolates before
 * interpolating it into a localized sentence (docs/03 §6: mixed-direction
 * strings use bidi isolation — an Arabic filename inside an English sentence,
 * or a "1.2 GB" size inside an Arabic one, must not scramble the prose
 * around it; names/sizes themselves are data and stay LTR).
 */
export function isolate(value: string): string {
  return `\u2066${value}\u2069`;
}

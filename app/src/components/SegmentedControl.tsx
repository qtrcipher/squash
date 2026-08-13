import { useTranslation } from "react-i18next";
import { nextSegmentIndex } from "../a11y";

export interface SegmentOption {
  value: string;
  label: string;
}

/**
 * Segmented control (format/preset/batch mode) as an APG radio group: one
 * tab stop (the checked segment), arrow keys move selection *and* focus
 * following the visual direction (RTL-aware, docs/03 §6), Home/End jump to
 * the ends. `autoFocus` marks the checked segment as the sheet's initial
 * focus target (Sheet focuses `[data-autofocus]` on open).
 */
export default function SegmentedControl({
  labelId,
  options,
  value,
  onChange,
  autoFocus = false,
}: {
  labelId: string;
  options: readonly SegmentOption[];
  value: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
}) {
  const { i18n } = useTranslation();
  const anyChecked = options.some((option) => option.value === value);

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const current = options.findIndex((option) => option.value === value);
    const next = nextSegmentIndex(
      current < 0 ? 0 : current,
      event.key,
      i18n.dir() === "rtl",
      options.length,
    );
    if (next === null) return;
    event.preventDefault();
    onChange(options[next].value);
    event.currentTarget
      .querySelectorAll<HTMLElement>('[role="radio"]')
      [next]?.focus();
  };

  return (
    <div
      className="segmented"
      role="radiogroup"
      aria-labelledby={labelId}
      onKeyDown={onKeyDown}
    >
      {options.map((option, index) => {
        const checked = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={checked}
            tabIndex={checked || (index === 0 && !anyChecked) ? 0 : -1}
            data-autofocus={autoFocus && checked ? "" : undefined}
            className={checked ? "segment active" : "segment"}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

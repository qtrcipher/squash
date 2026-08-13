import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import ar from "./locales/ar.json";

export const SUPPORTED_LANGUAGES = ["en", "ar"] as const;
export type Language = (typeof SUPPORTED_LANGUAGES)[number];

/** RTL languages get `dir="rtl"` on the document (docs/03 §6). */
export function applyDirection(language: string) {
  const dir = language === "ar" ? "rtl" : "ltr";
  document.documentElement.dir = dir;
  document.documentElement.lang = language;
}

void i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    ar: { translation: ar },
  },
  lng: "en", // settings store lands later; EN fallback per docs/06
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

applyDirection(i18n.language);
i18n.on("languageChanged", applyDirection);

export default i18n;

import { describe, expect, it } from "vitest";
import en from "./locales/en.json";
import ar from "./locales/ar.json";

type Messages = Record<string, unknown>;

/** Recursively collect dot-paths of leaf keys. */
function leafKeys(obj: Messages, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === "object") {
      return leafKeys(value as Messages, path);
    }
    return [path];
  });
}

describe("i18n locale parity (EN/AR from day one, house rule)", () => {
  it("en and ar define exactly the same keys", () => {
    expect(leafKeys(ar).sort()).toEqual(leafKeys(en).sort());
  });

  it("no empty translations", () => {
    for (const [locale, messages] of Object.entries({ en, ar })) {
      for (const key of leafKeys(messages)) {
        const value = key
          .split(".")
          .reduce<unknown>((acc, part) => (acc as Messages)[part], messages);
        expect(value, `${locale}:${key}`).toBeTypeOf("string");
        expect((value as string).trim().length, `${locale}:${key}`).toBeGreaterThan(0);
      }
    }
  });
});

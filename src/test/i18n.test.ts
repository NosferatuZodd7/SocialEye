import { describe, expect, it } from "vitest";
import { en, pt, translate } from "../lib/i18n";

describe("i18n", () => {
  it("Portuguese has every English key and no empty strings", () => {
    expect(Object.keys(pt).sort()).toEqual(Object.keys(en).sort());
    for (const [k, v] of Object.entries(pt)) expect(v.trim(), k).not.toBe("");
  });
  it("keeps the same placeholders in both languages", () => {
    const vars = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort();
    for (const k of Object.keys(en) as (keyof typeof en)[]) expect(vars(pt[k]), k).toEqual(vars(en[k]));
  });
  it("interpolates", () => {
    expect(translate("en", "acc.synced", { n: 5 })).toBe("Synced 5 items");
    expect(translate("pt", "acc.synced", { n: 5 })).toBe("5 itens sincronizados");
  });
});

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { loadAppSettings, patchAppSettings } from "../hooks/useSettingsBridge";
import { en, type MessageKey } from "./en";
import { zhCN } from "./zh-CN";

export type Locale = "en" | "zh-CN";
export function normalizeLocale(value: unknown): Locale { return value === "zh-CN" ? value : "en"; }
export function translate(locale: unknown, key: MessageKey, values: Record<string, string | number> = {}) {
  const message = normalizeLocale(locale) === "zh-CN" ? zhCN[key] : en[key];
  return message.replace(/\{(\w+)\}/g, (match, name: string) => name in values ? String(values[name]) : match);
}
const I18nContext = createContext({ locale: "en" as Locale, setLocale: async (_locale: Locale) => {} });
export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, updateLocale] = useState<Locale>("en");
  useEffect(() => {
    let disposed = false;
    void loadAppSettings().then(settings => { if (!disposed) updateLocale(normalizeLocale(settings.locale)); }).catch(() => {});
    return () => { disposed = true; };
  }, []);
  useEffect(() => { document.documentElement.lang = locale; }, [locale]);
  const setLocale = async (next: Locale) => {
    const previous = locale;
    updateLocale(next);
    try { await patchAppSettings({ locale: next }); }
    catch (error) { updateLocale(previous); throw error; }
  };
  return <I18nContext.Provider value={{ locale, setLocale }}>{children}</I18nContext.Provider>;
}
export const useLocale = () => useContext(I18nContext);
export function useT() {
  const { locale } = useLocale();
  return (key: MessageKey, values?: Record<string, string | number>) => translate(locale, key, values);
}

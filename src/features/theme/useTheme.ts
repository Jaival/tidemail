import { useEffect, useState } from "react";

export type ThemePref = "system" | "dark" | "light";
const KEY = "tidemail.theme";

/** Follows the Windows light/dark setting unless the user picked one in the palette. */
export function useTheme() {
  const [pref, setPref] = useState<ThemePref>(() => {
    const saved = localStorage.getItem(KEY);
    return saved === "dark" || saved === "light" ? saved : "system";
  });
  useEffect(() => {
    localStorage.setItem(KEY, pref);
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = pref === "system" ? (media.matches ? "dark" : "light") : pref;
    };
    apply();
    if (pref !== "system") return;
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [pref]);
  return [pref, setPref] as const;
}

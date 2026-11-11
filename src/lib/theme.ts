// Theme follows Windows (prefers-color-scheme) with a manual override. The choice is not
// sensitive, so it lives in localStorage (readable before the vault is unlocked).

export type ThemeChoice = "system" | "dark" | "light";
const KEY = "loki-theme";

export function getTheme(): ThemeChoice {
  try {
    const v = localStorage.getItem(KEY);
    return v === "dark" || v === "light" ? v : "system";
  } catch {
    return "system";
  }
}

export function applyTheme(choice: ThemeChoice): void {
  const root = document.documentElement;
  if (choice === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", choice);
}

export function setTheme(choice: ThemeChoice): void {
  try {
    if (choice === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, choice);
  } catch {
    /* storage can be unavailable; the choice still applies for this session */
  }
  applyTheme(choice);
}

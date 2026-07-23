export type Theme = "light" | "dark"

const THEME_STORAGE_KEY = "auq-wizard-theme"

export function resolveTheme(): Theme {
  const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY)
  if (storedTheme === "light" || storedTheme === "dark") return storedTheme

  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light"
}

export function applyTheme(theme: Theme) {
  document.documentElement.classList.toggle("dark", theme === "dark")
  document.documentElement.classList.toggle("light", theme === "light")
}

export function storeTheme(theme: Theme) {
  window.localStorage.setItem(THEME_STORAGE_KEY, theme)
}

import { Moon, Sun } from "lucide-react"
import { useLayoutEffect, useState } from "react"

import { Button } from "@/components/ui/button"
import { applyTheme, resolveTheme, storeTheme, type Theme } from "@/lib/theme"

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(resolveTheme)
  const nextTheme = theme === "dark" ? "light" : "dark"

  useLayoutEffect(() => applyTheme(theme), [theme])

  return (
    <Button
      type="button"
      variant="outline"
      size="icon-sm"
      aria-label={`Switch to ${nextTheme} mode`}
      title={`Switch to ${nextTheme} mode`}
      onClick={() => {
        storeTheme(nextTheme)
        setTheme(nextTheme)
      }}
    >
      {theme === "dark" ? <Sun /> : <Moon />}
    </Button>
  )
}

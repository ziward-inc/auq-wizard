import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { applyTheme, resolveTheme } from "@/lib/theme"

import App from "./App.tsx"
import "./index.css"

applyTheme(resolveTheme())

const rootElement = document.getElementById("root")

if (!rootElement) throw new Error("Missing #root element")

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

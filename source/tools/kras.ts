import { tool } from "@opencode-ai/plugin"
import { applyKrasDarkMode, lookupKras } from "../scripts/kras-lookup.ts"

export const lookup = tool({
  description: "Look up a KRAS parcel address.",
  args: {
    address: tool.schema.string().trim().min(1),
    darkMode: tool.schema.boolean().default(false),
  },
  execute: async ({ address, darkMode }) => lookupKras(address, darkMode),
})

export const dark_mode = tool({
  description: "Apply dark mode to the KRAS page.",
  args: {},
  execute: async () => applyKrasDarkMode(),
})

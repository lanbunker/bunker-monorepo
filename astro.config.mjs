import cloudflare from "@astrojs/cloudflare"
import react from "@astrojs/react"
import tailwindcss from "@tailwindcss/vite"
// @ts-check
import { defineConfig } from "astro/config"

export default defineConfig({
    output: "server",
    adapter: cloudflare(),
    integrations: [react()],
    vite: {
        plugins: [tailwindcss()],
    },
})

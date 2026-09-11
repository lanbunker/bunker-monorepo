// @ts-check
import cloudflare from "@astrojs/cloudflare"
import react from "@astrojs/react"
import tailwindcss from "@tailwindcss/vite"
import { defineConfig, envField } from "astro/config"

export default defineConfig({
    output: "server",
    adapter: cloudflare(),

    // Astro 7 compresses with JSX whitespace rules by default, which glues two
    // inline elements that only a line break separates. The site relies on that
    // break in many places, so it keeps the HTML-aware compression.
    compressHTML: true,

    env: {
        schema: {
            // Where the Rust API answers. A Worker var in production, the shell or
            // `.env` locally, and the `make dev` port when nothing is set.
            API_URL: envField.string({
                context: "server",
                access: "secret",
                default: "http://127.0.0.1:3000",
            }),
        },
    },

    vite: {
        plugins: [tailwindcss()],
    },

    integrations: [react()],
})

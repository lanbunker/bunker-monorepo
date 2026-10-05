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
            // Where the Rust API answers. Every environment of `wrangler.jsonc`
            // sets it, so a run that selects none fails instead of calling a
            // guessed port.
            API_URL: envField.string({ context: "server", access: "secret" }),
            // The key the API knows this site by. A local run has none, because
            // the API in local mode asks for no key.
            API_KEY: envField.string({
                context: "server",
                access: "secret",
                optional: true,
            }),
            // Cloudflare Turnstile on the signup form. `dev` and `e2e` use the
            // published test keys, which always pass. The site key is public,
            // but `secret` access reads it at runtime, so a build needs no value.
            TURNSTILE_SITE_KEY: envField.string({ context: "server", access: "secret" }),
            TURNSTILE_SECRET_KEY: envField.string({
                context: "server",
                access: "secret",
            }),
        },
    },

    vite: {
        // A build rewrites the dependency cache, and a running dev server then
        // fails to load the files it optimized, so `astro dev` keeps its own.
        cacheDir: process.argv.slice(2).includes("dev")
            ? "node_modules/.vite-dev"
            : undefined,
        plugins: [tailwindcss()],
    },

    integrations: [react()],
})

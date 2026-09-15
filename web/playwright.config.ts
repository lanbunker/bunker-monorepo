import { defineConfig } from "@playwright/test"

// Both servers start from scratch: the API on its own database file under
// `.dev/`, so a run never touches the development data, and the site pointed at
// that API. The API binary compiles against `.env` first, then runs elsewhere.
// The API port must match `env.e2e.vars.API_URL` in `web/wrangler.jsonc`.
const API_PORT = 3999
const WEB_PORT = 4399

export default defineConfig({
    testDir: "./e2e",
    fullyParallel: false,
    workers: 1,
    retries: 0,
    timeout: 30_000,
    expect: { timeout: 10_000 },
    reporter: [["list"]],
    use: {
        baseURL: `http://127.0.0.1:${WEB_PORT}`,
        // A failure keeps its trace, so a run on another machine can be replayed.
        trace: "retain-on-failure",
    },
    webServer: [
        {
            command: `cd .. && rm -f .dev/e2e.db .dev/e2e.db-wal .dev/e2e.db-shm && cargo build -q -p bunker-api && APP_ENV=test PORT=${API_PORT} DATABASE_URL='sqlite://.dev/e2e.db?mode=rwc' ./target/debug/bunker-api`,
            url: `http://127.0.0.1:${API_PORT}/health/ready`,
            reuseExistingServer: false,
            timeout: 180_000,
            // The output of both servers goes into the run log, so a failure on
            // another machine shows the server side of the story.
            stdout: "pipe",
            stderr: "pipe",
        },
        {
            // The site under test is the production build, served by the same
            // runtime that serves it on Cloudflare. The dev server runs the code
            // through a module runner instead, and on Linux that runner hangs on
            // the first request to an action route, so the bracket editor never
            // got an answer in CI. A Worker var beats the shell in local mode, so
            // the API URL comes from the `e2e` environment of `wrangler.jsonc`,
            // which holds this same port. `astro preview` detaches itself when it
            // detects an agent, and then Playwright cannot stop it: the variable
            // turns that detection off, whatever its value.
            command: `CLOUDFLARE_ENV=e2e pnpm exec astro build && CLOUDFLARE_ENV=e2e ASTRO_PREVIEW_BACKGROUND=0 pnpm exec astro preview --host 127.0.0.1 --port ${WEB_PORT}`,
            url: `http://127.0.0.1:${WEB_PORT}/`,
            reuseExistingServer: false,
            timeout: 240_000,
            stdout: "pipe",
            stderr: "pipe",
        },
    ],
})

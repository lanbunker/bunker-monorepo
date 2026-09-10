import { defineConfig } from "@playwright/test"

// Both servers start from scratch: the API on its own database file under
// `.dev/`, so a run never touches the development data, and the site pointed at
// that API. The API binary compiles against `.env` first, then runs elsewhere.
const API_PORT = 3999
const WEB_PORT = 4399

export default defineConfig({
    testDir: "./e2e",
    fullyParallel: false,
    workers: 1,
    retries: 0,
    timeout: 30_000,
    // The dev server compiles a page on its first visit, which takes seconds.
    expect: { timeout: 10_000 },
    reporter: [["list"]],
    use: {
        baseURL: `http://127.0.0.1:${WEB_PORT}`,
    },
    webServer: [
        {
            command: `cd .. && rm -f .dev/e2e.db .dev/e2e.db-wal .dev/e2e.db-shm && cargo build -q -p bunker-api && APP_ENV=test PORT=${API_PORT} DATABASE_URL='sqlite://.dev/e2e.db?mode=rwc' ./target/debug/bunker-api`,
            url: `http://127.0.0.1:${API_PORT}/health/ready`,
            reuseExistingServer: false,
            timeout: 180_000,
        },
        {
            command: `API_URL=http://127.0.0.1:${API_PORT} pnpm astro dev --host 127.0.0.1 --port ${WEB_PORT}`,
            url: `http://127.0.0.1:${WEB_PORT}/`,
            reuseExistingServer: false,
            timeout: 120_000,
        },
    ],
})

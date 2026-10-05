import { TURNSTILE_SECRET_KEY } from "astro:env/server"
import { z } from "zod"

const VERIFY_URL = "https://challenges.cloudflare.com/turnstile/v0/siteverify"

/** Cloudflare answers in well under a second. A hang must not hold the form. */
const TIMEOUT_MS = 5_000

/**
 * The answer of siteverify. A test secret answers no `action` and says so in
 * `metadata`, so only a real secret can hold the action to its value.
 */
const verdict = z.object({
    success: z.boolean(),
    "error-codes": z.array(z.string()).optional(),
    action: z.string().optional(),
    metadata: z.object({ result_with_testing_key: z.boolean().optional() }).optional(),
})

type Verdict = z.infer<typeof verdict>

/**
 * `refused` is a token that Cloudflare does not accept. `unavailable` is a check
 * that did not happen: a timeout, an outage or an answer of another shape.
 */
export type BotCheck = "passed" | "refused" | "unavailable"

const ask = async (token: string, ip: string | null): Promise<unknown> => {
    const response = await fetch(VERIFY_URL, {
        method: "POST",
        body: new URLSearchParams({
            secret: TURNSTILE_SECRET_KEY,
            response: token,
            ...(ip ? { remoteip: ip } : {}),
        }),
        signal: AbortSignal.timeout(TIMEOUT_MS),
    })
    if (!response.ok) throw new Error(`siteverify answered ${response.status}`)
    return response.json()
}

const judge = (answer: Verdict, action: string): BotCheck => {
    if (!answer.success) {
        const codes = answer["error-codes"] ?? []
        console.error("turnstile refused the token", codes)
        // Cloudflare documents `internal-error` as its own fault, worth a retry.
        return codes.includes("internal-error") ? "unavailable" : "refused"
    }
    const testing = answer.metadata?.result_with_testing_key === true
    if (!testing && answer.action !== action) {
        console.error("turnstile token is for another action", answer.action)
        return "refused"
    }
    return "passed"
}

/**
 * What Cloudflare says of the token the widget put in the form. A token made by
 * the widget of another form (`action`) counts as a refusal.
 */
export const checkTurnstile = (
    token: string,
    ip: string | null,
    action: string,
): Promise<BotCheck> =>
    ask(token, ip)
        .then(body => judge(verdict.parse(body), action))
        .catch((error: unknown) => {
            console.error("turnstile check unavailable", error)
            return "unavailable" as const
        })

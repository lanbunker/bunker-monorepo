import { isInputError } from "astro:actions"
import type { ActionError } from "astro:actions"

/** What `Astro.getActionResult` answers, reduced to the part a page reads. */
export type ActionOutcome = { error?: ActionError | undefined } | undefined

const FALLBACK = "Check the form and send it again."

/** Every field message of a refused input, in field order. */
const fieldMessages = (error: ActionError): string[] =>
    isInputError(error)
        ? Object.values(error.fields).flatMap(messages => messages ?? [])
        : []

/**
 * The sentence a form shows for a failure.
 *
 * Astro reports a refused input as an `ActionInputError` whose `message` is the
 * whole issue list as JSON. That string is unreadable on a page, so the field
 * messages are read back and the first one is shown. Every other failure keeps
 * the message the action wrote, which is the message the API wrote.
 */
export const errorMessage = (error: ActionError | undefined): string | undefined => {
    if (!error) return undefined
    if (!isInputError(error)) return error.message
    return fieldMessages(error).at(0) ?? FALLBACK
}

/**
 * The first failure of a page that renders several forms. Astro answers at most
 * one action per request, so at most one of these holds an error.
 */
export const firstError = (...outcomes: ActionOutcome[]): ActionError | undefined =>
    outcomes.find(outcome => outcome?.error)?.error

/**
 * The path a page redirects to after a successful action. The key is a known
 * word, and `who` carries the handle the API answered. A reader of the page can
 * still write any `who` by hand, so the page renders it as text only.
 */
export const donePath = (base: string, done: string, who?: string): string =>
    `${base}?${new URLSearchParams(who === undefined ? { done } : { done, who })}`

/**
 * Picks the notice for a `?done=` redirect. The query string is anyone's to
 * write, so only a known key renders.
 */
export const noticeFrom = (
    url: URL,
    notices: Readonly<Record<string, string>>,
): string | undefined => {
    const key = url.searchParams.get("done")
    // A plain lookup would also find `valueOf` or `constructor` on the prototype.
    return key !== null && Object.hasOwn(notices, key) ? notices[key] : undefined
}

import { API_URL } from "astro:env/server"
import createClient from "openapi-fetch"

import type { components, paths } from "./api-types"

export type Player = components["schemas"]["Player"]
export type Glyph = components["schemas"]["Glyph"]
export type ApiErrorBody = components["schemas"]["ApiErrorBody"]
export type ErrorCode = components["schemas"]["ErrorCode"]
export type Tournament = components["schemas"]["Tournament"]
export type TournamentDetail = components["schemas"]["TournamentDetail"]
export type TournamentStatus = components["schemas"]["TournamentStatus"]
export type Entrant = components["schemas"]["Entrant"]
export type Bracket = components["schemas"]["Bracket"]
export type Match = components["schemas"]["Match"]

/** The cookie that holds the bearer token. HttpOnly, so scripts never see it. */
export const SESSION_COOKIE = "bunker_session"

/** Shown when the API answered nothing a client can act on. */
export const UNREACHABLE = "The API is unreachable."

export type ApiClient = ReturnType<typeof createClient<paths>>

/**
 * A typed client. Every path, body and response comes from `openapi.json`.
 * Call it through `call` or `callEmpty`, never on its own: those two turn a
 * dead network into a value instead of an exception.
 */
export const apiClient = (token?: string): ApiClient =>
    createClient<paths>({
        baseUrl: API_URL,
        headers: token ? { authorization: `Bearer ${token}` } : undefined,
    })

/** Why a call carries no data. */
export type ApiFailure =
    | { readonly kind: "unreachable" }
    | {
          readonly kind: "refused"
          readonly status: number
          readonly body: ApiErrorBody | undefined
      }

export type ApiResult<T> =
    | { readonly ok: true; readonly data: T }
    | { readonly ok: false; readonly failure: ApiFailure }

/** The shape `openapi-fetch` answers with, narrowed to what a caller reads. */
type Outcome<T = unknown> = { data?: T | undefined; error?: unknown; response: Response }

/**
 * Every code the API can answer with. The `satisfies` clause fails to compile
 * when `make api-types` adds a code, so the map can never fall behind.
 */
const ERROR_CODES = {
    GenericError: true,
    ServiceUnavailable: true,
    ItemNotFound: true,
    HandleTaken: true,
    InvalidCredentials: true,
    Unauthorized: true,
    Forbidden: true,
    WrongPassword: true,
    RegistrationClosed: true,
    InvalidState: true,
    NotAnEntrant: true,
    InvalidRequest: true,
    UnprocessableRequest: true,
    UnsupportedMediaType: true,
    PayloadTooLarge: true,
    RouteNotFound: true,
    MethodNotAllowed: true,
} satisfies Record<ErrorCode, true>

/** A rejected call answers the thrown value, which carries no response. */
const isOutcome = <T>(value: unknown): value is Outcome<T> =>
    typeof value === "object" &&
    value !== null &&
    "response" in value &&
    value.response instanceof Response

const isErrorCode = (value: unknown): value is ErrorCode =>
    typeof value === "string" && Object.hasOwn(ERROR_CODES, value)

/**
 * A body from the wire is data, not a promise. A proxy or a crash can answer
 * anything, so every field is checked before it reaches a page.
 */
const errorBody = (value: unknown): ApiErrorBody | undefined => {
    if (typeof value !== "object" || value === null) return undefined
    if (!("code" in value) || !("message" in value) || !("status" in value)) {
        return undefined
    }
    if (!isErrorCode(value.code)) return undefined
    if (typeof value.message !== "string") return undefined
    if (typeof value.status !== "number") return undefined
    const cause = "cause" in value && typeof value.cause === "string" ? value.cause : null
    return { code: value.code, message: value.message, status: value.status, cause }
}

/**
 * The site cannot reach the API. The cause never reaches a page, so it goes to
 * the server log: an outage that leaves no trace is an outage nobody can fix.
 */
const unreachable = (cause: unknown): ApiResult<never> => {
    console.error("api call failed", cause)
    return { ok: false, failure: { kind: "unreachable" } }
}

const refused = (response: Response, error: unknown): ApiResult<never> => ({
    ok: false,
    failure: { kind: "refused", status: response.status, body: errorBody(error) },
})

/**
 * Runs one call and answers a value in every case. A refusal keeps its status
 * and its body, and a dead network becomes `unreachable`. Nothing throws, so a
 * page or an action never turns an outage into a 500.
 */
export const call = async <T>(
    run: (client: ApiClient) => Promise<Outcome<T>>,
    token?: string,
): Promise<ApiResult<T>> => {
    const outcome = await run(apiClient(token)).catch(cause => cause)
    if (!isOutcome<T>(outcome)) return unreachable(outcome)
    if (!outcome.response.ok) return refused(outcome.response, outcome.error)

    const data = outcome.data
    // A 2xx with no body breaks the contract of a typed route. Reporting it as
    // a refusal would carry a 2xx status into the answer, so it counts as an
    // outage: the caller sees the same thing and the cause reaches the log.
    if (data === undefined) {
        return unreachable(
            new Error(`empty body on a typed route: ${outcome.response.url}`),
        )
    }
    return { ok: true, data }
}

/** The same as `call` for a route that answers 204 with no body. */
export const callEmpty = async (
    run: (client: ApiClient) => Promise<Outcome>,
    token?: string,
): Promise<ApiResult<void>> => {
    const outcome = await run(apiClient(token)).catch(cause => cause)
    if (!isOutcome(outcome)) return unreachable(outcome)
    if (!outcome.response.ok) return refused(outcome.response, outcome.error)
    return { ok: true, data: undefined }
}

/** The data of a successful call, or `undefined`. For a page that renders both. */
export const dataOf = <T>(result: ApiResult<T>): T | undefined =>
    result.ok ? result.data : undefined

export const failureOf = <T>(result: ApiResult<T>): ApiFailure | undefined =>
    result.ok ? undefined : result.failure

export const codeOf = (failure: ApiFailure): ErrorCode | undefined =>
    failure.kind === "refused" ? failure.body?.code : undefined

/** The message the API wrote for the client, or a fallback when it is down. */
export const messageOf = (failure: ApiFailure, fallback = UNREACHABLE): string =>
    failure.kind === "refused" ? (failure.body?.message ?? fallback) : fallback

/**
 * True when the resource does not exist. A page turns this into a 404 and keeps
 * a real outage on the page, so a down API never looks like a dead link.
 */
export const isMissing = (failure: ApiFailure): boolean =>
    failure.kind === "refused" && failure.status >= 400 && failure.status < 500

import { ActionError } from "astro:actions"
import type { ActionErrorCode } from "astro:actions"
import { match } from "ts-pattern"

import { call, codeOf, messageOf } from "./api"
import type { ApiFailure, ApiResult, ErrorCode, Player } from "./api"
import { adminSessionOf, sessionOf } from "./session"

/**
 * The status a refusal with no body becomes. Something in front of the API
 * answered, such as a Cloudflare error page, so the status is all there is.
 */
const codeForStatus = (status: number): ActionErrorCode =>
    match(status)
        .with(404, () => "NOT_FOUND" as const)
        .with(413, () => "CONTENT_TOO_LARGE" as const)
        .with(429, () => "TOO_MANY_REQUESTS" as const)
        .when(
            value => value >= 500,
            () => "BAD_GATEWAY" as const,
        )
        .otherwise(() => "BAD_REQUEST" as const)

/**
 * The status an API code becomes for the browser. The API already decided the
 * HTTP status, but an action answers through Astro, so the code is mapped one
 * time here instead of at every call site.
 */
const codeFor = (code: ErrorCode | undefined, status: number): ActionErrorCode =>
    match(code)
        .with("GenericError", () => "INTERNAL_SERVER_ERROR" as const)
        .with("ServiceUnavailable", () => "SERVICE_UNAVAILABLE" as const)
        .with("ItemNotFound", "RouteNotFound", () => "NOT_FOUND" as const)
        .with(
            "HandleTaken",
            "RegistrationClosed",
            "CheckinClosed",
            "InvalidState",
            "NotAnEntrant",
            () => "CONFLICT" as const,
        )
        .with(
            "InvalidCredentials",
            "Unauthorized",
            "WrongPassword",
            () => "UNAUTHORIZED" as const,
        )
        .with("Forbidden", "PasswordChangeRequired", () => "FORBIDDEN" as const)
        .with("InvalidRequest", () => "BAD_REQUEST" as const)
        .with("UnprocessableRequest", () => "UNPROCESSABLE_CONTENT" as const)
        .with("UnsupportedMediaType", () => "UNSUPPORTED_MEDIA_TYPE" as const)
        .with("PayloadTooLarge", () => "CONTENT_TOO_LARGE" as const)
        .with("MethodNotAllowed", () => "METHOD_NOT_ALLOWED" as const)
        .with(undefined, () => codeForStatus(status))
        .exhaustive()

/** The API refused or is down. Its message is the one the caller can act on. */
const apiError = (failure: ApiFailure): ActionError =>
    new ActionError({
        code: match(failure)
            .with({ kind: "unreachable" }, () => "SERVICE_UNAVAILABLE" as const)
            .with({ kind: "refused" }, refusal =>
                codeFor(codeOf(refusal), refusal.status),
            )
            .exhaustive(),
        message: messageOf(failure),
    })

/**
 * Unwraps a call inside an action. A failure leaves the handler as the matching
 * `ActionError`, so no handler builds an error response of its own.
 */
export const unwrap = <T>(result: ApiResult<T>): T => {
    if (!result.ok) throw apiError(result.failure)
    return result.data
}

/** The token of the caller. Every write needs one. */
export const requireToken = (locals: App.Locals): string => {
    const session = sessionOf(locals)
    if (!session) {
        throw new ActionError({ code: "UNAUTHORIZED", message: "Log in first." })
    }
    return session.token
}

export const requireAdmin = (locals: App.Locals): string => {
    const session = adminSessionOf(locals)
    if (!session) {
        throw new ActionError({ code: "FORBIDDEN", message: "Admins only." })
    }
    return session.token
}

/** The player behind a handle an admin typed. An unknown handle refuses the action. */
export const playerByHandle = async (handle: string, token: string): Promise<Player> =>
    unwrap(
        await call(
            client =>
                client.GET("/api/players/{handle}", { params: { path: { handle } } }),
            token,
        ),
    )

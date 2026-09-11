import { ActionError } from "astro:actions"
import type { ActionErrorCode } from "astro:actions"
import { match } from "ts-pattern"

import { codeOf, messageOf } from "./api"
import type { ApiFailure, ApiResult, ErrorCode } from "./api"

/**
 * The status an API code becomes for the browser. The API already decided the
 * HTTP status, but an action answers through Astro, so the code is mapped one
 * time here instead of at every call site.
 */
const codeFor = (code: ErrorCode | undefined): ActionErrorCode =>
    match(code)
        .with("HandleTaken", () => "CONFLICT" as const)
        .with("ItemNotFound", "RouteNotFound", () => "NOT_FOUND" as const)
        .with(
            "InvalidCredentials",
            "Unauthorized",
            "WrongPassword",
            () => "UNAUTHORIZED" as const,
        )
        .with("Forbidden", () => "FORBIDDEN" as const)
        .with("ServiceUnavailable", () => "SERVICE_UNAVAILABLE" as const)
        .with("PayloadTooLarge", () => "CONTENT_TOO_LARGE" as const)
        .with("UnsupportedMediaType", () => "UNSUPPORTED_MEDIA_TYPE" as const)
        .with("MethodNotAllowed", () => "METHOD_NOT_ALLOWED" as const)
        .with("UnprocessableRequest", () => "UNPROCESSABLE_CONTENT" as const)
        .otherwise(() => "BAD_REQUEST" as const)

/** The API refused or is down. Its message is the one the caller can act on. */
export const apiError = (failure: ApiFailure): ActionError =>
    new ActionError({
        code: match(failure)
            .with({ kind: "unreachable" }, () => "SERVICE_UNAVAILABLE" as const)
            .with({ kind: "refused" }, refusal => codeFor(codeOf(refusal)))
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
    if (!locals.token) {
        throw new ActionError({ code: "UNAUTHORIZED", message: "Log in first." })
    }
    return locals.token
}

/** The token of an admin caller. */
export const requireAdmin = (locals: App.Locals): string => {
    if (locals.player?.role !== "admin" || !locals.token) {
        throw new ActionError({ code: "FORBIDDEN", message: "Admins only." })
    }
    return locals.token
}

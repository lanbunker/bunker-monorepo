import { API_URL } from "astro:env/server"
import createClient from "openapi-fetch"

import type { components, paths } from "./api-types"

export type Player = components["schemas"]["Player"]
export type Glyph = components["schemas"]["Glyph"]
type ApiErrorBody = components["schemas"]["ApiErrorBody"]

/** The cookie that holds the bearer token. HttpOnly, so scripts never see it. */
export const SESSION_COOKIE = "bunker_session"

/** A typed client. Every path, body and response comes from openapi.json. */
export const apiClient = (token?: string) =>
    createClient<paths>({
        baseUrl: API_URL,
        headers: token ? { authorization: `Bearer ${token}` } : undefined,
    })

/** The message the API wrote for the client, or a fallback when it is down. */
export const errorMessage = (error: ApiErrorBody | undefined, fallback: string): string =>
    error?.message ?? fallback

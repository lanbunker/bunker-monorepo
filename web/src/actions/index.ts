import type { ActionAPIContext } from "astro:actions"
import { defineAction } from "astro:actions"

import { requireAdmin, requireToken, unwrap } from "../lib/action"
import { SESSION_COOKIE, call, callEmpty } from "../lib/api"
import type { ApiResult, TokenResponse } from "../lib/api"
import {
    adjustmentInput,
    byId,
    credentials,
    handleInput,
    passwordChangeInput,
    playerHandleInput,
    roleInput,
    signupInput,
} from "../lib/schemas"
import { eventActions } from "./events"
import { tournamentActions } from "./tournaments"

/** Sets the session cookie until the token expires. */
const storeSession = (context: ActionAPIContext, token: string, expiresAt: string) => {
    context.cookies.set(SESSION_COOKIE, token, {
        httpOnly: true,
        sameSite: "lax",
        // Secure over a secure connection, so production sets it and a local run
        // over http, the e2e suite included, does not. `import.meta.env.PROD` is
        // true for every build, e2e included, so it cannot decide this.
        secure: context.url.protocol === "https:",
        path: "/",
        expires: new Date(expiresAt),
    })
}

/**
 * Keeps the session a signup or a login answered. Both forms go back to where
 * the player came from, so both answer the same value.
 */
const openSession = (
    context: ActionAPIContext,
    input: { handle: string; next?: string | undefined },
    result: ApiResult<TokenResponse>,
) => {
    const session = unwrap(result)
    storeSession(context, session.token, session.expiresAt)
    return { handle: input.handle, next: input.next }
}

export const server = {
    ...eventActions,
    ...tournamentActions,

    signup: defineAction({
        accept: "form",
        input: signupInput,
        handler: async (input, context) =>
            openSession(
                context,
                input,
                await call(client =>
                    client.POST("/api/auth/signup", {
                        body: { handle: input.handle, password: input.password },
                    }),
                ),
            ),
    }),

    login: defineAction({
        accept: "form",
        input: credentials,
        handler: async (input, context) =>
            openSession(
                context,
                input,
                await call(client =>
                    client.POST("/api/auth/login", {
                        body: { handle: input.handle, password: input.password },
                    }),
                ),
            ),
    }),

    logout: defineAction({
        accept: "form",
        handler: (_input, context) => {
            context.cookies.delete(SESSION_COOKIE, { path: "/" })
            return { ok: true }
        },
    }),

    changePassword: defineAction({
        accept: "form",
        input: passwordChangeInput,
        handler: async (input, context) => {
            const session = unwrap(
                await call(
                    client =>
                        client.POST("/api/me/password", {
                            body: {
                                currentPassword: input.currentPassword,
                                newPassword: input.newPassword,
                            },
                        }),
                    requireToken(context.locals),
                ),
            )
            // The API revoked every older token, this session included. Keep the
            // player in.
            storeSession(context, session.token, session.expiresAt)
            return { ok: true }
        },
    }),

    changeHandle: defineAction({
        accept: "form",
        input: handleInput,
        handler: async (input, context) => {
            const player = unwrap(
                await call(
                    client =>
                        client.PUT("/api/me/handle", { body: { handle: input.handle } }),
                    requireToken(context.locals),
                ),
            )
            return { handle: player.handle }
        },
    }),

    renamePlayer: defineAction({
        accept: "form",
        input: playerHandleInput,
        handler: async (input, context) => {
            const player = unwrap(
                await call(
                    client =>
                        client.PUT("/api/admin/players/{id}/handle", {
                            params: { path: { id: input.id } },
                            body: { handle: input.handle },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { handle: player.handle }
        },
    }),

    resetPassword: defineAction({
        accept: "form",
        input: byId,
        handler: async (input, context) => {
            const reset = unwrap(
                await call(
                    client =>
                        client.POST("/api/admin/players/{id}/password-reset", {
                            params: { path: { id: input.id } },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { temporaryPassword: reset.temporaryPassword }
        },
    }),

    setRole: defineAction({
        accept: "form",
        input: roleInput,
        handler: async (input, context) => {
            const player = unwrap(
                await call(
                    client =>
                        client.PATCH("/api/admin/players/{id}", {
                            params: { path: { id: input.id } },
                            body: { role: input.role },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { handle: player.handle, role: player.role }
        },
    }),

    adjustCycles: defineAction({
        accept: "form",
        input: adjustmentInput,
        handler: async (input, context) => {
            const entry = unwrap(
                await call(
                    client =>
                        client.POST("/api/admin/players/{id}/cycles", {
                            params: { path: { id: input.id } },
                            body: { amount: input.amount, note: input.note },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            // The entry names no player, so the handle is the one the form carried.
            return { handle: input.handle, amount: entry.amount }
        },
    }),

    deletePlayer: defineAction({
        accept: "form",
        input: byId,
        handler: async (input, context) => {
            unwrap(
                await callEmpty(
                    client =>
                        client.DELETE("/api/admin/players/{id}", {
                            params: { path: { id: input.id } },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { ok: true }
        },
    }),
}

import type { AstroCookies } from "astro"
import { defineAction } from "astro:actions"
import { z } from "zod"

import { requireAdmin, requireToken, unwrap } from "../lib/action"
import { SESSION_COOKIE, call, callEmpty } from "../lib/api"
import {
    credentials,
    handle,
    passwordChangeInput,
    signupInput,
    uuid,
} from "../lib/schemas"
import { tournamentActions } from "./tournaments"

/** Sets the session cookie until the token expires. */
const storeSession = (cookies: AstroCookies, token: string, expiresAt: string) => {
    cookies.set(SESSION_COOKIE, token, {
        httpOnly: true,
        sameSite: "lax",
        secure: import.meta.env.PROD,
        path: "/",
        expires: new Date(expiresAt),
    })
}

export const server = {
    ...tournamentActions,

    signup: defineAction({
        accept: "form",
        input: signupInput,
        handler: async (input, context) => {
            const session = unwrap(
                await call(client =>
                    client.POST("/api/auth/signup", {
                        body: { handle: input.handle, password: input.password },
                    }),
                ),
            )
            storeSession(context.cookies, session.token, session.expiresAt)
            return { handle: input.handle }
        },
    }),

    login: defineAction({
        accept: "form",
        input: credentials,
        handler: async (input, context) => {
            const session = unwrap(
                await call(client => client.POST("/api/auth/login", { body: input })),
            )
            storeSession(context.cookies, session.token, session.expiresAt)
            return { handle: input.handle }
        },
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
            storeSession(context.cookies, session.token, session.expiresAt)
            return { ok: true }
        },
    }),

    changeHandle: defineAction({
        accept: "form",
        input: z.object({ handle }),
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
        input: z.object({ id: uuid, handle }),
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
        input: z.object({ id: uuid }),
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
        input: z.object({ id: uuid, role: z.enum(["user", "admin"]) }),
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

    deletePlayer: defineAction({
        accept: "form",
        input: z.object({ id: uuid }),
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

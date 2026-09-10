import type { AstroCookies } from "astro"
import { ActionError, defineAction } from "astro:actions"
import { z } from "zod"

import { SESSION_COOKIE, apiClient, errorMessage } from "../lib/api"
import type { ApiErrorBody } from "../lib/api"

const handleField = z.string().min(3).max(20)

const credentials = z.object({
    handle: handleField,
    password: z.string().min(8).max(128),
})

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

/** A rename fails the same way for a player and for an admin. */
const handleFailure = (error: ApiErrorBody | undefined) =>
    new ActionError({
        code: error?.code === "HandleTaken" ? "CONFLICT" : "BAD_REQUEST",
        message: errorMessage(error, "The API is unreachable."),
    })

const requireAdmin = (locals: App.Locals): string => {
    if (locals.player?.role !== "admin" || !locals.token) {
        throw new ActionError({ code: "FORBIDDEN", message: "Admins only." })
    }
    return locals.token
}

export const server = {
    signup: defineAction({
        accept: "form",
        input: credentials
            .extend({ confirmPassword: z.string().min(8).max(128) })
            .refine(input => input.password === input.confirmPassword, {
                message: "The two passwords differ.",
                path: ["confirmPassword"],
            }),
        handler: async (input, context) => {
            const { data, error } = await apiClient().POST("/api/auth/signup", {
                body: { handle: input.handle, password: input.password },
            })
            if (!data) {
                throw new ActionError({
                    code: error?.code === "HandleTaken" ? "CONFLICT" : "BAD_REQUEST",
                    message: errorMessage(error, "The API is unreachable."),
                })
            }
            storeSession(context.cookies, data.token, data.expiresAt)
            return { handle: input.handle }
        },
    }),

    login: defineAction({
        accept: "form",
        input: credentials,
        handler: async (input, context) => {
            const { data, error } = await apiClient().POST("/api/auth/login", {
                body: input,
            })
            if (!data) {
                throw new ActionError({
                    code: "UNAUTHORIZED",
                    message: errorMessage(error, "The API is unreachable."),
                })
            }
            storeSession(context.cookies, data.token, data.expiresAt)
            return { handle: input.handle }
        },
    }),

    logout: defineAction({
        accept: "form",
        handler: async (_input, context) => {
            context.cookies.delete(SESSION_COOKIE, { path: "/" })
            return { ok: true }
        },
    }),

    changePassword: defineAction({
        accept: "form",
        input: z
            .object({
                currentPassword: z.string().min(8).max(128),
                newPassword: z.string().min(8).max(128),
                confirmPassword: z.string().min(8).max(128),
            })
            .refine(input => input.newPassword === input.confirmPassword, {
                message: "The two new passwords differ.",
                path: ["confirmPassword"],
            }),
        handler: async (input, context) => {
            const token = context.locals.token
            if (!token)
                throw new ActionError({ code: "UNAUTHORIZED", message: "Log in first." })
            const { data, error } = await apiClient(token).POST("/api/me/password", {
                body: {
                    currentPassword: input.currentPassword,
                    newPassword: input.newPassword,
                },
            })
            if (!data) {
                throw new ActionError({
                    code: "BAD_REQUEST",
                    message: errorMessage(error, "The API is unreachable."),
                })
            }
            // The API revoked every older token, this session included. Keep the player in.
            storeSession(context.cookies, data.token, data.expiresAt)
            return { ok: true }
        },
    }),

    changeHandle: defineAction({
        accept: "form",
        input: z.object({ handle: handleField }),
        handler: async (input, context) => {
            const token = context.locals.token
            if (!token)
                throw new ActionError({ code: "UNAUTHORIZED", message: "Log in first." })
            const { data, error } = await apiClient(token).PUT("/api/me/handle", {
                body: { handle: input.handle },
            })
            if (!data) throw handleFailure(error)
            return { handle: data.handle }
        },
    }),

    renamePlayer: defineAction({
        accept: "form",
        input: z.object({ id: z.uuid(), handle: handleField }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).PUT(
                "/api/admin/players/{id}/handle",
                {
                    params: { path: { id: input.id } },
                    body: { handle: input.handle },
                },
            )
            if (!data) throw handleFailure(error)
            return { handle: data.handle }
        },
    }),

    resetPassword: defineAction({
        accept: "form",
        input: z.object({ id: z.uuid() }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).POST(
                "/api/admin/players/{id}/password-reset",
                { params: { path: { id: input.id } } },
            )
            if (!data) {
                throw new ActionError({
                    code: "BAD_REQUEST",
                    message: errorMessage(error, "The API is unreachable."),
                })
            }
            return { temporaryPassword: data.temporaryPassword }
        },
    }),

    setRole: defineAction({
        accept: "form",
        input: z.object({
            id: z.uuid(),
            role: z.enum(["user", "admin"]),
        }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).PATCH(
                "/api/admin/players/{id}",
                {
                    params: { path: { id: input.id } },
                    body: { role: input.role },
                },
            )
            if (!data) {
                throw new ActionError({
                    code: "BAD_REQUEST",
                    message: errorMessage(error, "The API is unreachable."),
                })
            }
            return { handle: data.handle, role: data.role }
        },
    }),

    deletePlayer: defineAction({
        accept: "form",
        input: z.object({ id: z.uuid() }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { error, response } = await apiClient(token).DELETE(
                "/api/admin/players/{id}",
                { params: { path: { id: input.id } } },
            )
            if (!response.ok) {
                throw new ActionError({
                    code: "BAD_REQUEST",
                    message: errorMessage(error, "The API is unreachable."),
                })
            }
            return { ok: true }
        },
    }),
}

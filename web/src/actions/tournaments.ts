import { ActionError, defineAction } from "astro:actions"
import { z } from "zod"

import { apiClient, errorMessage } from "../lib/api"
import type { ApiErrorBody } from "../lib/api"

const uuid = z.uuid()

const requireToken = (locals: App.Locals): string => {
    if (!locals.token)
        throw new ActionError({ code: "UNAUTHORIZED", message: "Log in first." })
    return locals.token
}

const requireAdmin = (locals: App.Locals): string => {
    if (locals.player?.role !== "admin" || !locals.token) {
        throw new ActionError({ code: "FORBIDDEN", message: "Admins only." })
    }
    return locals.token
}

/** The API refused or is down. Its message is the one the admin can act on. */
const failure = (error: ApiErrorBody | undefined) =>
    new ActionError({
        code: error?.code === "ItemNotFound" ? "NOT_FOUND" : "BAD_REQUEST",
        message: errorMessage(error, "The API is unreachable."),
    })

const tournamentFields = {
    name: z.string().trim().min(1).max(60),
    game: z.string().trim().min(1).max(40),
    mode: z.string().trim().min(1).max(30),
    // An empty textarea arrives as null from a form post.
    description: z
        .string()
        .max(1000)
        .nullish()
        .transform(value => (value ?? "").trim()),
    date: z.iso.date(),
    registrationClosesAt: z.iso.datetime({ offset: true }),
}

/** Player and admin actions on tournaments. Forms post, the bracket island calls. */
export const tournamentActions = {
    applyToTournament: defineAction({
        accept: "form",
        input: z.object({ id: uuid }),
        handler: async (input, context) => {
            const token = requireToken(context.locals)
            const { data, error } = await apiClient(token).POST(
                "/api/tournaments/{id}/registration",
                {
                    params: { path: { id: input.id } },
                },
            )
            if (!data) throw failure(error)
            return { id: input.id }
        },
    }),

    retireFromTournament: defineAction({
        accept: "form",
        input: z.object({ id: uuid }),
        handler: async (input, context) => {
            const token = requireToken(context.locals)
            const { error, response } = await apiClient(token).DELETE(
                "/api/tournaments/{id}/registration",
                {
                    params: { path: { id: input.id } },
                },
            )
            if (!response.ok) throw failure(error)
            return { id: input.id }
        },
    }),

    createTournament: defineAction({
        accept: "form",
        input: z.object(tournamentFields),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).POST(
                "/api/admin/tournaments",
                { body: input },
            )
            if (!data) throw failure(error)
            return { id: data.id }
        },
    }),

    updateTournament: defineAction({
        accept: "form",
        input: z.object({ id: uuid, ...tournamentFields }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const body = { ...input, id: undefined }
            const { data, error } = await apiClient(token).PATCH(
                "/api/admin/tournaments/{id}",
                {
                    params: { path: { id: input.id } },
                    body,
                },
            )
            if (!data) throw failure(error)
            return { id: input.id }
        },
    }),

    setTournamentStatus: defineAction({
        accept: "form",
        input: z.object({
            id: uuid,
            status: z.enum(["open", "live", "concluded"]),
            // The select is absent on most forms and its empty option is "".
            // Both mean no winner.
            winner: z
                .string()
                .nullish()
                .transform(value => (value ? value : undefined))
                .pipe(uuid.optional()),
        }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).POST(
                "/api/admin/tournaments/{id}/status",
                {
                    params: { path: { id: input.id } },
                    body: { status: input.status, winner: input.winner ?? null },
                },
            )
            if (!data) throw failure(error)
            return { id: input.id, status: data.status }
        },
    }),

    deleteTournament: defineAction({
        accept: "form",
        input: z.object({ id: uuid }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { error, response } = await apiClient(token).DELETE(
                "/api/admin/tournaments/{id}",
                {
                    params: { path: { id: input.id } },
                },
            )
            if (!response.ok) throw failure(error)
            return { deleted: true }
        },
    }),

    addEntrant: defineAction({
        accept: "form",
        input: z.object({ id: uuid, handle: z.string().trim().min(3).max(20) }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const player = await apiClient(token).GET("/api/players/{handle}", {
                params: { path: { handle: input.handle } },
            })
            if (!player.data) throw failure(player.error)
            const { data, error } = await apiClient(token).POST(
                "/api/admin/tournaments/{id}/entrants",
                {
                    params: { path: { id: input.id } },
                    body: { playerId: player.data.id },
                },
            )
            if (!data) throw failure(error)
            return { handle: player.data.handle }
        },
    }),

    removeEntrant: defineAction({
        accept: "form",
        input: z.object({ id: uuid, entrantId: uuid }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { error, response } = await apiClient(token).DELETE(
                "/api/admin/tournaments/{id}/entrants/{entrantId}",
                { params: { path: { id: input.id, entrantId: input.entrantId } } },
            )
            if (!response.ok) throw failure(error)
            return { removed: true }
        },
    }),

    generateBracket: defineAction({
        input: z.object({ id: uuid }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).POST(
                "/api/admin/tournaments/{id}/bracket",
                {
                    params: { path: { id: input.id } },
                },
            )
            if (!data) throw failure(error)
            return data
        },
    }),

    deleteBracket: defineAction({
        input: z.object({ id: uuid }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { error, response } = await apiClient(token).DELETE(
                "/api/admin/tournaments/{id}/bracket",
                {
                    params: { path: { id: input.id } },
                },
            )
            if (!response.ok) throw failure(error)
            return { deleted: true }
        },
    }),

    reorderSeeds: defineAction({
        input: z.object({ id: uuid, entrants: z.array(uuid).min(2) }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).PUT(
                "/api/admin/tournaments/{id}/seeds",
                {
                    params: { path: { id: input.id } },
                    body: { entrants: input.entrants },
                },
            )
            if (!data) throw failure(error)
            return data
        },
    }),

    reportResult: defineAction({
        input: z.object({ id: uuid, matchId: uuid, winner: uuid }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).PUT(
                "/api/admin/tournaments/{id}/matches/{matchId}/result",
                {
                    params: { path: { id: input.id, matchId: input.matchId } },
                    body: { winner: input.winner },
                },
            )
            if (!data) throw failure(error)
            return data
        },
    }),

    clearResult: defineAction({
        input: z.object({ id: uuid, matchId: uuid }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const { data, error } = await apiClient(token).DELETE(
                "/api/admin/tournaments/{id}/matches/{matchId}/result",
                { params: { path: { id: input.id, matchId: input.matchId } } },
            )
            if (!data) throw failure(error)
            return data
        },
    }),
}

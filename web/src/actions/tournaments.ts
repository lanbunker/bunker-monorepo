import { defineAction } from "astro:actions"
import { z } from "zod"

import { requireAdmin, requireToken, unwrap } from "../lib/action"
import { call, callEmpty } from "../lib/api"
import {
    handle,
    statusChangeInput,
    tournamentFields,
    tournamentInput,
    uuid,
} from "../lib/schemas"

const byId = z.object({ id: uuid })

/** Player and admin actions on tournaments. Forms post, the bracket island calls. */
export const tournamentActions = {
    applyToTournament: defineAction({
        accept: "form",
        input: byId,
        handler: async (input, context) => {
            unwrap(
                await call(
                    client =>
                        client.POST("/api/tournaments/{id}/registration", {
                            params: { path: { id: input.id } },
                        }),
                    requireToken(context.locals),
                ),
            )
            return { id: input.id }
        },
    }),

    retireFromTournament: defineAction({
        accept: "form",
        input: byId,
        handler: async (input, context) => {
            unwrap(
                await callEmpty(
                    client =>
                        client.DELETE("/api/tournaments/{id}/registration", {
                            params: { path: { id: input.id } },
                        }),
                    requireToken(context.locals),
                ),
            )
            return { id: input.id }
        },
    }),

    createTournament: defineAction({
        accept: "form",
        input: tournamentInput,
        handler: async (input, context) => {
            const created = unwrap(
                await call(
                    client => client.POST("/api/admin/tournaments", { body: input }),
                    requireAdmin(context.locals),
                ),
            )
            return { id: created.id }
        },
    }),

    updateTournament: defineAction({
        accept: "form",
        input: z.object({ id: uuid, ...tournamentFields }),
        handler: async (input, context) => {
            unwrap(
                await call(
                    client =>
                        client.PATCH("/api/admin/tournaments/{id}", {
                            params: { path: { id: input.id } },
                            body: {
                                name: input.name,
                                game: input.game,
                                mode: input.mode,
                                description: input.description,
                                date: input.date,
                                registrationClosesAt: input.registrationClosesAt,
                            },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { id: input.id }
        },
    }),

    setTournamentStatus: defineAction({
        accept: "form",
        input: statusChangeInput,
        handler: async (input, context) => {
            const tournament = unwrap(
                await call(
                    client =>
                        client.POST("/api/admin/tournaments/{id}/status", {
                            params: { path: { id: input.id } },
                            body: { status: input.status, winner: input.winner ?? null },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { id: input.id, status: tournament.status }
        },
    }),

    deleteTournament: defineAction({
        accept: "form",
        input: byId,
        handler: async (input, context) => {
            unwrap(
                await callEmpty(
                    client =>
                        client.DELETE("/api/admin/tournaments/{id}", {
                            params: { path: { id: input.id } },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { deleted: true }
        },
    }),

    addEntrant: defineAction({
        accept: "form",
        input: z.object({ id: uuid, handle }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const player = unwrap(
                await call(
                    client =>
                        client.GET("/api/players/{handle}", {
                            params: { path: { handle: input.handle } },
                        }),
                    token,
                ),
            )
            unwrap(
                await call(
                    client =>
                        client.POST("/api/admin/tournaments/{id}/entrants", {
                            params: { path: { id: input.id } },
                            body: { playerId: player.id },
                        }),
                    token,
                ),
            )
            return { handle: player.handle }
        },
    }),

    removeEntrant: defineAction({
        accept: "form",
        input: z.object({ id: uuid, entrantId: uuid }),
        handler: async (input, context) => {
            unwrap(
                await callEmpty(
                    client =>
                        client.DELETE(
                            "/api/admin/tournaments/{id}/entrants/{entrantId}",
                            {
                                params: {
                                    path: { id: input.id, entrantId: input.entrantId },
                                },
                            },
                        ),
                    requireAdmin(context.locals),
                ),
            )
            return { removed: true }
        },
    }),

    generateBracket: defineAction({
        input: byId,
        handler: async (input, context) =>
            unwrap(
                await call(
                    client =>
                        client.POST("/api/admin/tournaments/{id}/bracket", {
                            params: { path: { id: input.id } },
                        }),
                    requireAdmin(context.locals),
                ),
            ),
    }),

    deleteBracket: defineAction({
        input: byId,
        handler: async (input, context) => {
            unwrap(
                await callEmpty(
                    client =>
                        client.DELETE("/api/admin/tournaments/{id}/bracket", {
                            params: { path: { id: input.id } },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { deleted: true }
        },
    }),

    reorderSeeds: defineAction({
        input: z.object({ id: uuid, entrants: z.array(uuid).min(2) }),
        handler: async (input, context) =>
            unwrap(
                await call(
                    client =>
                        client.PUT("/api/admin/tournaments/{id}/seeds", {
                            params: { path: { id: input.id } },
                            body: { entrants: input.entrants },
                        }),
                    requireAdmin(context.locals),
                ),
            ),
    }),

    reportResult: defineAction({
        input: z.object({ id: uuid, matchId: uuid, winner: uuid }),
        handler: async (input, context) =>
            unwrap(
                await call(
                    client =>
                        client.PUT(
                            "/api/admin/tournaments/{id}/matches/{matchId}/result",
                            {
                                params: {
                                    path: { id: input.id, matchId: input.matchId },
                                },
                                body: { winner: input.winner },
                            },
                        ),
                    requireAdmin(context.locals),
                ),
            ),
    }),

    clearResult: defineAction({
        input: z.object({ id: uuid, matchId: uuid }),
        handler: async (input, context) =>
            unwrap(
                await call(
                    client =>
                        client.DELETE(
                            "/api/admin/tournaments/{id}/matches/{matchId}/result",
                            {
                                params: {
                                    path: { id: input.id, matchId: input.matchId },
                                },
                            },
                        ),
                    requireAdmin(context.locals),
                ),
            ),
    }),
}

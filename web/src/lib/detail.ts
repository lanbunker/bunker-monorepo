import { z } from "zod"

import type { TournamentDetail } from "./api"

/** The twelve palette colors. The API never sends another one. */
const GLYPH_COLORS = [
    "#ffb000",
    "#4fd1e0",
    "#b48cff",
    "#ff6b57",
    "#9dff57",
    "#ff5cc8",
    "#cfe7ff",
    "#ffd75c",
    "#2dd4bf",
    "#ff8c42",
    "#7cc4ff",
    "#7ef5c0",
] as const

const glyph = z.object({
    bits: z.number(),
    color: z.enum(GLYPH_COLORS),
})

const rank = z.enum(["zombie", "guest", "user", "sudoer", "daemon", "kernel"])

const standing = z.object({
    cycles: z.number(),
    place: z.number(),
    players: z.number(),
    rank,
    floor: z.number(),
    next: z.object({ rank, floor: z.number() }).nullable().optional(),
})

const player = z.object({
    id: z.string(),
    handle: z.string(),
    role: z.enum(["user", "admin"]),
    createdAt: z.string(),
    glyph,
    standing,
})

const entrant = z.object({
    id: z.string(),
    player: player.nullable().optional(),
    seed: z.number().nullable().optional(),
    skill: z.number().nullable().optional(),
    registeredAt: z.string(),
})

const match = z.object({
    id: z.string(),
    round: z.number(),
    slot: z.number(),
    entrantA: z.string().nullable().optional(),
    entrantB: z.string().nullable().optional(),
    winner: z.string().nullable().optional(),
})

const bracket = z.object({
    rounds: z.array(z.array(match)),
})

const tournament = z.object({
    id: z.string(),
    name: z.string(),
    game: z.string(),
    mode: z.string(),
    description: z.string(),
    date: z.string(),
    status: z.enum(["draft", "open", "live", "concluded"]),
    registrationClosesAt: z.string(),
    entrantCount: z.number(),
    hasBracket: z.boolean(),
    winner: entrant.nullable().optional(),
    createdAt: z.string(),
})

const detail = z.object({
    tournament,
    entrants: z.array(entrant),
    bracket: bracket.nullable().optional(),
})

/**
 * The detail, or `undefined` when the body is not one. The kiosk reads it from a
 * site route, so it arrives over the wire and is data, not a type.
 *
 * The return type is the contract check against `openapi.json`, and it is only
 * half of one. A `z.object` drops keys it does not declare, so a field the API
 * adds passes unseen. Only a new required field, or a changed type, fails to
 * compile after `make api-types`.
 */
export const parseTournamentDetail = (body: unknown): TournamentDetail | undefined => {
    const parsed = detail.safeParse(body)
    return parsed.success ? parsed.data : undefined
}

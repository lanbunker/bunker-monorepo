/**
 * MOCK: hardcoded players for the hidden profile preview.
 * UI purpose only. No backend, no login, no persistence. Replace with D1 queries later.
 */

import { generateGlyph } from "../lib/glyph"
import type { StoredGlyph } from "../lib/glyph"

export type KarmaBreakdown = {
    scores: number
    tournaments: number
    attendance: number
    crew: number
}

export type BestScore = {
    game: string
    score: number
    position: number
    karma: number
}

export type TournamentResult = {
    name: string
    /** DD/MM/YYYY */
    date: string
    position: number
    participants: number
}

export type Player = {
    handle: string
    /** Generated once at signup and stored on the players row. */
    glyph: StoredGlyph
    /** DD/MM/YYYY of the first attended session */
    enlisted: string
    enlistedAt: string
    karma: KarmaBreakdown
    /** Badge ids from src/data/badges.ts */
    badges: string[]
    bestScores: BestScore[]
    tournaments: TournamentResult[]
    lastSeen: string
}

export const players: Player[] = [
    {
        handle: "dave",
        glyph: generateGlyph("dave"),
        enlisted: "21/06/2025",
        enlistedAt: "FPS ARENA LAN PARTY",
        karma: { scores: 1010, tournaments: 690, attendance: 160, crew: 280 },
        badges: ["founder", "crew", "champion", "full-attendance", "record-holder"],
        bestScores: [
            { game: "galaga", score: 412_330, position: 1, karma: 25 },
            { game: "ms_pac_man", score: 186_540, position: 2, karma: 18 },
            { game: "donkey_kong", score: 312_700, position: 3, karma: 14 },
            { game: "dig_dug", score: 94_220, position: 5, karma: 9 },
            { game: "r_type", score: 228_100, position: 7, karma: 6 },
        ],
        tournaments: [
            {
                name: "Quake 3 Arena: FFA",
                date: "21/06/2025",
                position: 2,
                participants: 16,
            },
            {
                name: "Mario Kart 8: Grand Prix",
                date: "28/10/2025",
                position: 4,
                participants: 12,
            },
            {
                name: "COD Black Ops 2: 1v1 Gun Game",
                date: "26/02/2026",
                position: 1,
                participants: 20,
            },
        ],
        lastSeen: "cab03 @theoffice · 12 min ago",
    },
    {
        handle: "ziopera",
        glyph: generateGlyph("ziopera"),
        enlisted: "28/10/2025",
        enlistedAt: "BUNKER//SESSION 02",
        badges: ["podium", "night-owl", "record-holder"],
        karma: { scores: 620, tournaments: 300, attendance: 120, crew: 150 },
        bestScores: [
            { game: "galaga", score: 388_120, position: 2, karma: 18 },
            { game: "street_fighter_2", score: 51_900, position: 1, karma: 25 },
        ],
        tournaments: [
            {
                name: "Mario Kart 8: Grand Prix",
                date: "28/10/2025",
                position: 2,
                participants: 12,
            },
            {
                name: "COD Black Ops 2: 1v1 Gun Game",
                date: "26/02/2026",
                position: 3,
                participants: 20,
            },
        ],
        lastSeen: "cab01 @theoffice · 2 h ago",
    },
    {
        handle: "ciccio",
        glyph: generateGlyph("ciccio"),
        enlisted: "28/10/2025",
        enlistedAt: "BUNKER//SESSION 02",
        badges: ["first-blood"],
        karma: { scores: 310, tournaments: 130, attendance: 80, crew: 0 },
        bestScores: [{ game: "galaga", score: 301_450, position: 3, karma: 14 }],
        tournaments: [
            {
                name: "COD Black Ops 2: 1v1 Gun Game",
                date: "26/02/2026",
                position: 9,
                participants: 20,
            },
        ],
        lastSeen: "3 d ago",
    },
    {
        handle: "Fede_88",
        glyph: generateGlyph("Fede_88"),
        enlisted: "17/02/2026",
        enlistedAt: "BUNKER//SESSION 03",
        badges: ["rookie"],
        karma: { scores: 190, tournaments: 60, attendance: 80, crew: 0 },
        bestScores: [{ game: "galaga", score: 254_900, position: 4, karma: 12 }],
        tournaments: [
            {
                name: "COD Black Ops 2: 1v1 Gun Game",
                date: "26/02/2026",
                position: 12,
                participants: 20,
            },
        ],
        lastSeen: "1 w ago",
    },
    {
        handle: "vale_98",
        glyph: generateGlyph("vale_98"),
        enlisted: "17/02/2026",
        enlistedAt: "BUNKER//SESSION 03",
        badges: [],
        karma: { scores: 50, tournaments: 0, attendance: 40, crew: 0 },
        bestScores: [{ game: "galaga", score: 201_770, position: 5, karma: 9 }],
        tournaments: [],
        lastSeen: "17 feb 2026",
    },
    {
        handle: "gabri.exe",
        glyph: generateGlyph("gabri.exe"),
        enlisted: "21/06/2025",
        enlistedAt: "FPS ARENA LAN PARTY",
        badges: ["founder", "champion", "night-owl"],
        karma: { scores: 480, tournaments: 260, attendance: 120, crew: 180 },
        bestScores: [
            { game: "galaga", score: 188_300, position: 6, karma: 7 },
            { game: "quake_3", score: 41, position: 1, karma: 25 },
        ],
        tournaments: [
            {
                name: "Quake 3 Arena: FFA",
                date: "21/06/2025",
                position: 1,
                participants: 16,
            },
            {
                name: "COD Black Ops 2: 1v1 Gun Game",
                date: "26/02/2026",
                position: 5,
                participants: 20,
            },
        ],
        lastSeen: "cab02 @theoffice · 40 min ago",
    },
    {
        handle: "mortadella",
        glyph: generateGlyph("mortadella"),
        enlisted: "24/10/2026",
        enlistedAt: "BUNKER//SESSION 04",
        badges: [],
        karma: { scores: 20, tournaments: 0, attendance: 40, crew: 0 },
        bestScores: [{ game: "galaga", score: 140_020, position: 7, karma: 4 }],
        tournaments: [],
        lastSeen: "now",
    },
]

export const totalKarma = (p: Player): number =>
    p.karma.scores + p.karma.tournaments + p.karma.attendance + p.karma.crew

export const findPlayer = (handle: string): Player | undefined =>
    players.find(p => p.handle.toLowerCase() === handle.toLowerCase())

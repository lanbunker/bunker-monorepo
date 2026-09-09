import type { ImageMetadata } from "astro"

import winnerCodFeb26 from "../assets/images/winners/winner-cod-feb26.webp"

export type Tournament = {
    name: string
    game: string
    description?: string
    /** DD/MM/YYYY */
    date: string
    format: string
    participants?: number
    winner?: string
    winnerImage?: ImageMetadata
    formLink?: string
    /** UTC ISO datetime, for example "2026-05-10T20:00:00Z" */
    openUntil?: string
    status: "OPEN" | "CLOSED"
}

export const tournaments: Tournament[] = [
    {
        name: "1v1 Sniper Only",
        game: "COD Modern Warfare 2",
        description: "1v1 Sniper Only on 'Rust' map, direct elimination bracket.",
        date: "24/10/2026",
        format: "1v1 direct elimination",
        formLink:
            "https://docs.google.com/forms/d/e/1FAIpQLSfJknBekQGU0dAodeJlWL-rnnZAxMMZRTVaAqOV4yVSbkl1yA/viewform?usp=dialog",
        openUntil: "2026-10-23T21:59:59Z",
        status: "OPEN",
    },
    {
        name: "1v1 Gun Game",
        game: "COD Black Ops 2",
        description:
            "1v1 Gun Game on 'Hijack' map, UAV always active, -3 weapons on melee death.",
        date: "26/02/2026",
        format: "1v1 direct elimination",
        participants: 20,
        winner: "Marino",
        winnerImage: winnerCodFeb26,
        status: "CLOSED",
    },
]

// Tournaments sort newest first, so the last open entry is the soonest one.
export const upcomingTournament = tournaments.findLast(t => t.status === "OPEN")

/** True while the tournament accepts sign-ups: status OPEN and the deadline, if any, is in the future. */
export const isRegistrationOpen = (t: Tournament, now = Date.now()): boolean =>
    t.status === "OPEN" && (!t.openUntil || new Date(t.openUntil).getTime() > now)

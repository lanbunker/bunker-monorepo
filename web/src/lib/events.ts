import { match } from "ts-pattern"

import type { Event } from "./api"
import type { BadgeTone } from "./styles"

export type EventPhase = "upcoming" | "live" | "past"

/** Where a night sits against the clock. The API decides the door; this only sorts the list. */
export const phaseOf = (event: Event, now = Date.now()): EventPhase => {
    if (now < new Date(event.startsAt).getTime()) return "upcoming"
    if (now < new Date(event.endsAt).getTime()) return "live"
    return "past"
}

/** The list comes latest night first, so the soonest one still to come is the last such item. */
export const nextEvent = (items: readonly Event[], now = Date.now()): Event | undefined =>
    items.findLast(event => phaseOf(event, now) !== "past")

/** The first night ever, for the uptime counter. */
export const firstEvent = (items: readonly Event[]): Event | undefined => items.at(-1)

/** The badge of a card. Tonight is the loud one. */
export const phaseBadge = (
    phase: EventPhase,
): { label: string; tone: BadgeTone; filled: boolean } =>
    match(phase)
        .with("live", () => ({ label: "TONIGHT", tone: "warn" as const, filled: true }))
        .with("upcoming", () => ({
            label: "ANNOUNCED",
            tone: "accent" as const,
            filled: true,
        }))
        .with("past", () => ({
            label: "ARCHIVED",
            tone: "muted" as const,
            filled: false,
        }))
        .exhaustive()

/** The path of the door on this site, from a check-in code. */
export const checkinPath = (code: string): string => `/checkin/${code}`

/** The path of the door as a QR code needs it: absolute, on this site. */
export const checkinUrl = (origin: string, code: string): string =>
    `${origin}${checkinPath(code)}`

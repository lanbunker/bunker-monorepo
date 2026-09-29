import type {
    CyclesRules,
    KindTotal,
    PointEntry,
    PointKind,
    Rank,
    Standing,
    TierRule,
} from "./api"

export type RankInfo = {
    /** Position on the ladder, 0 for the bottom. It is the number of lit cells. */
    tier: number
    label: string
    /** Tailwind text class. The color tokens live in global.css. */
    text: string
}

/**
 * The look of each rank. The API owns the thresholds and sends them on every
 * standing, so the site only names and colors the tiers.
 */
export const RANKS: Record<Rank, RankInfo> = {
    zombie: {
        tier: 0,
        label: "ZOMBIE",
        text: "text-rank-zombie",
    },
    guest: {
        tier: 1,
        label: "GUEST",
        text: "text-rank-guest",
    },
    user: { tier: 2, label: "USER", text: "text-rank-user" },
    sudoer: {
        tier: 3,
        label: "SUDOER",
        text: "text-rank-sudoer",
    },
    daemon: {
        tier: 4,
        label: "DAEMON",
        text: "text-rank-daemon",
    },
    kernel: {
        tier: 5,
        label: "KERNEL",
        text: "text-rank-kernel",
    },
}

/** The number of cells in a rank mark: every rank above the bottom lights one. */
export const RANK_CELLS = 5

export type Source = "events" | "tournaments" | "adjustments"

/** Where cycles come from, as a player thinks of it: several kinds make one source. */
const SOURCE_OF: Record<PointKind, Source> = {
    checkin: "events",
    tournament_entry: "tournaments",
    match_win: "tournaments",
    semifinalist: "tournaments",
    finalist: "tournaments",
    champion: "tournaments",
    adjustment: "adjustments",
}

export const SOURCES: readonly Source[] = ["events", "tournaments", "adjustments"]

/** The cycles of each source, every source present, from the totals by kind. */
export const sourceTotals = (totals: readonly KindTotal[]): Record<Source, number> => {
    const sumOf = (source: Source) =>
        totals
            .filter(total => SOURCE_OF[total.kind] === source)
            .reduce((sum, total) => sum + total.cycles, 0)
    return {
        events: sumOf("events"),
        tournaments: sumOf("tournaments"),
        adjustments: sumOf("adjustments"),
    }
}

/**
 * The places the board marks out. A player earns the podium, so a roster where
 * nobody has cycles yet has no podium at all.
 */
export const isPodium = (standing: Standing): boolean =>
    standing.place <= 3 && standing.cycles > 0

/** `1,240` or `-80`: a comma every three digits, and the sign of a loss stays. */
export const formatCycles = (cycles: number): string =>
    new Intl.NumberFormat("en-US").format(cycles)

/** `+120` or `-50`, so a gain is never mistaken for a plain number. */
export const signed = (amount: number): string =>
    amount < 0 ? formatCycles(amount) : `+${formatCycles(amount)}`

/** The cycles still missing for the next rank. `undefined` at the top of the ladder. */
export const toGo = (standing: Standing): number | undefined =>
    standing.next ? Math.max(0, standing.next.floor - standing.cycles) : undefined

/**
 * How far along the current rank the player is, from 0 to 1. At the top of the
 * ladder there is nothing left to fill.
 */
export const progressOf = (standing: Standing): number => {
    if (!standing.next) return 1
    const span = standing.next.floor - standing.floor
    return span <= 0
        ? 1
        : Math.min(1, Math.max(0, (standing.cycles - standing.floor) / span))
}

/** A bar such as `██████░░░░░░░░░░░░░░` of `width` cells. */
export const asciiBar = (fraction: number, width: number): string => {
    const filled = Math.round(Math.min(1, Math.max(0, fraction)) * width)
    return "█".repeat(filled) + "░".repeat(width - filled)
}

export type KindInfo = {
    label: string
    blurb: string
}

/** What each kind of entry is called. */
export const KINDS: Record<PointKind, KindInfo> = {
    checkin: { label: "event check-in", blurb: "scan the QR code at the entrance" },
    tournament_entry: { label: "tournament entry", blurb: "you showed up and played" },
    match_win: {
        label: "match won",
        blurb: "each bracket match you win. a bye is not a win",
    },
    semifinalist: { label: "semifinalist", blurb: "out in the semis" },
    finalist: { label: "finalist", blurb: "lost the final" },
    champion: { label: "champion", blurb: "won the cup" },
    adjustment: { label: "admin adjustment", blurb: "a bonus or a penalty, with a note" },
}

/** What one line of the history says, in words a player reads. */
export const entryLabel = (entry: PointEntry): string =>
    entry.kind === "adjustment"
        ? entry.amount < 0
            ? "admin penalty"
            : "admin bonus"
        : KINDS[entry.kind].label

/** "2-7", "8-15", "16+": the field a tier covers, from the rules. Short, it is a column head. */
export const tierLabel = (tiers: readonly TierRule[], index: number): string => {
    const tier = tiers[index]
    const next = tiers[index + 1]
    if (!tier) return ""
    return next ? `${tier.minEntrants}-${next.minEntrants - 1}` : `${tier.minEntrants}+`
}

/** "+40" for an amount that is the same in every field, "+120 to +300" otherwise. */
export const cyclesRange = (byTier: readonly number[]): string => {
    const low = Math.min(...byTier)
    const high = Math.max(...byTier)
    return low === high ? signed(low) : `${signed(low)} to ${signed(high)}`
}

/** The cycles a check-in pays. A door has no field of entrants, so every tier pays the same. */
export const checkinCycles = (rules: CyclesRules | undefined): number | undefined =>
    rules?.awards.find(award => award.kind === "checkin")?.cycles.at(0)

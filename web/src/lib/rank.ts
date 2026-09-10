export type Rank = {
    name: string
    level: number
    min: number
}

/** Karma ladder. Names follow Unix privilege levels. */
export const RANKS: Rank[] = [
    { name: "GUEST", level: 1, min: 0 },
    { name: "USER", level: 2, min: 250 },
    { name: "SUDO", level: 3, min: 800 },
    { name: "ROOT", level: 4, min: 1800 },
    { name: "DAEMON", level: 5, min: 3000 },
    { name: "KERNEL", level: 6, min: 5000 },
]

export type RankProgress = {
    current: Rank
    next?: Rank
    /** 0 to 1 inside the current band. 1 when no next rank exists. */
    progress: number
    toGo: number
}

export const rankFor = (karma: number): RankProgress => {
    const current = RANKS.findLast(r => karma >= r.min) ?? RANKS[0]
    const next = RANKS.find(r => r.min > current.min)
    const progress = next ? (karma - current.min) / (next.min - current.min) : 1
    return { current, next, progress, toGo: next ? next.min - karma : 0 }
}

/** Text meter such as [████░░░░░░] for terminal-style progress. */
export const meter = (progress: number, width = 12): string => {
    const filled = Math.round(Math.min(1, Math.max(0, progress)) * width)
    return `[${"█".repeat(filled)}${"░".repeat(width - filled)}]`
}

/**
 * MOCK: badge catalog for the hidden profile preview.
 * UI purpose only. Becomes the badges table when the backend exists.
 */

export type Badge = {
    id: string
    label: string
    /** One UTF-8 glyph that renders in every monospace font. */
    icon: string
    color: string
    description: string
}

export const badges: Badge[] = [
    {
        id: "founder",
        label: "founder",
        icon: "◆",
        color: "#ffb000",
        description: "present at the first session",
    },
    {
        id: "crew",
        label: "crew",
        icon: "⚙",
        color: "#4fd1e0",
        description: "helps run the bunker",
    },
    {
        id: "champion",
        label: "champion",
        icon: "★",
        color: "#ffd75c",
        description: "won a tournament",
    },
    {
        id: "podium",
        label: "podium",
        icon: "▲",
        color: "#b48cff",
        description: "top three in a tournament",
    },
    {
        id: "full-attendance",
        label: "full attendance",
        icon: "■",
        color: "#9dff57",
        description: "never missed a session",
    },
    {
        id: "first-blood",
        label: "first blood",
        icon: "✚",
        color: "#ff6b57",
        description: "first kill of a tournament",
    },
    {
        id: "night-owl",
        label: "night owl",
        icon: "☾",
        color: "#cfe7ff",
        description: "still playing at 4 am",
    },
    {
        id: "record-holder",
        label: "record holder",
        icon: "▮",
        color: "#ff5cc8",
        description: "holds an arcade record",
    },
    {
        id: "rookie",
        label: "rookie",
        icon: "○",
        color: "#9c9c9c",
        description: "first session in the bunker",
    },
]

export const findBadge = (id: string): Badge | undefined => badges.find(b => b.id === id)

export const GLYPH_SIZE = 5

/** Player colors. Each one reads on the dark background at small and large sizes. */
export const GLYPH_COLORS = [
    "#ffb000",
    "#4fd1e0",
    "#b48cff",
    "#ff6b57",
    "#9dff57",
    "#ff5cc8",
    "#cfe7ff",
    "#ffd75c",
] as const

export type GlyphColor = (typeof GLYPH_COLORS)[number]

/** What the players table stores: a 25 bit grid, row-major, bit 0 is top left, and a color. */
export type StoredGlyph = {
    bits: number
    color: GlyphColor
}

/** FNV-1a, 32 bit. Small and stable across runtimes. */
const hash = (input: string): number =>
    Array.from(input.toLowerCase()).reduce((h, ch) => {
        const mixed = (h ^ ch.charCodeAt(0)) >>> 0
        return Math.imul(mixed, 16777619) >>> 0
    }, 2166136261)

const CORNERS = [0, 4, 20, 24]

/**
 * Generate a glyph for a new account. Runs once at signup, then the result is stored.
 * The left three columns come from hash bits and mirror to the right.
 * The center column always has at least one cell so the mark has a spine.
 */
export const generateGlyph = (seed: string): StoredGlyph => {
    const h = hash(seed)
    const half = GLYPH_SIZE * 3
    const source = Array.from({ length: half }, (_, i) => ((h >>> (i % 32)) & 1) === 1)
    const centerLit = Array.from(
        { length: GLYPH_SIZE },
        (_, r) => source[r * 3 + 2],
    ).some(Boolean)
    const spine = centerLit ? source : source.map((b, i) => (i === 2 + 3 * 2 ? true : b))

    const cells = Array.from({ length: GLYPH_SIZE * GLYPH_SIZE }, (_, i) => {
        const row = Math.floor(i / GLYPH_SIZE)
        const col = i % GLYPH_SIZE
        const mirrored = col > 2 ? GLYPH_SIZE - 1 - col : col
        return spine[row * 3 + mirrored]
    })

    // A mark with fewer than seven cells looks like noise, so fill the corners for weight.
    const litCount = cells.filter(Boolean).length
    const weighted = litCount < 7 ? cells.map((c, i) => c || CORNERS.includes(i)) : cells

    return { bits: cellsToBits(weighted), color: GLYPH_COLORS[h % GLYPH_COLORS.length] }
}

export const cellsToBits = (cells: boolean[]): number =>
    cells.reduce((bits, lit, i) => (lit ? bits | (1 << i) : bits), 0)

/** Expand the stored integer into a row-major 5x5 grid for any renderer. */
export const bitsToCells = (bits: number): boolean[] =>
    Array.from({ length: GLYPH_SIZE * GLYPH_SIZE }, (_, i) => ((bits >>> i) & 1) === 1)

/** Five text rows such as "#.#.#", the portable form for terminals and JSON. */
export const bitsToRows = (bits: number): string[] => {
    const cells = bitsToCells(bits)
    return Array.from({ length: GLYPH_SIZE }, (_, r) =>
        cells
            .slice(r * GLYPH_SIZE, (r + 1) * GLYPH_SIZE)
            .map(lit => (lit ? "#" : "."))
            .join(""),
    )
}

/** The mark is a square grid of this many cells per side. */
export const GLYPH_SIZE = 5

/** One cell of the grid. The coordinate is its identity, not its position. */
export type GlyphCell = {
    readonly x: number
    readonly y: number
    readonly lit: boolean
}

/**
 * Expands the stored integer into a row-major grid. Bit 0 is the top left cell.
 * The API generates and stores the bits at signup. The site only draws them.
 */
export const bitsToCells = (bits: number): GlyphCell[] =>
    Array.from({ length: GLYPH_SIZE * GLYPH_SIZE }, (_, i) => ({
        x: i % GLYPH_SIZE,
        y: Math.floor(i / GLYPH_SIZE),
        lit: ((bits >>> i) & 1) === 1,
    }))

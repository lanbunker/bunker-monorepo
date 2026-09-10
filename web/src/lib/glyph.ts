/** The mark is a square grid of this many cells per side. */
export const GLYPH_SIZE = 5

/**
 * Expands the stored integer into a row-major grid. Bit 0 is the top left cell.
 * The API generates and stores the bits at signup. The site only draws them.
 */
export const bitsToCells = (bits: number): boolean[] =>
    Array.from({ length: GLYPH_SIZE * GLYPH_SIZE }, (_, i) => ((bits >>> i) & 1) === 1)

import type { Glyph } from "../lib/api"
import { GLYPH_SIZE, bitsToCells } from "../lib/glyph"

type GlyphMarkProps = {
    glyph: Glyph
    /** CSS size such as "1em" or "7.5rem". Width and height are equal. */
    size?: string
    framed?: boolean
    className?: string
}

/** The player mark as the API sends it: a 5x5 grid and a color. */
export const GlyphMark = (props: GlyphMarkProps) => {
    const size = props.size ?? "1em"
    const cells = bitsToCells(props.glyph.bits)
    // The frame keeps 12% padding on each side, so the mark itself takes 76% of the box.
    const markSize = props.framed ? `calc(${size} * 0.76)` : size
    const classes = ["inline-flex shrink-0 items-center justify-center align-[-0.15em]"]
    if (props.framed) classes.push("border")
    if (props.className) classes.push(props.className)

    return (
        <span
            className={classes.join(" ")}
            style={{
                width: size,
                height: size,
                color: props.glyph.color,
                borderColor: props.glyph.color,
            }}
            aria-hidden="true"
        >
            <svg
                viewBox={`0 0 ${GLYPH_SIZE} ${GLYPH_SIZE}`}
                shapeRendering="crispEdges"
                className="block"
                style={{ width: markSize, height: markSize }}
                fill="currentColor"
            >
                {cells.map(cell =>
                    cell.lit ? (
                        <rect
                            key={`${cell.x}-${cell.y}`}
                            x={cell.x}
                            y={cell.y}
                            width="1"
                            height="1"
                        />
                    ) : null,
                )}
            </svg>
        </span>
    )
}

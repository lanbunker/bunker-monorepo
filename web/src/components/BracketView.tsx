import { useState } from "react"

import type { Bracket, Entrant, Match } from "../lib/api"
import { entrantName, roundLabel } from "../lib/tournaments"
import { GlyphMark } from "./GlyphMark"

/**
 * What an editor may do on the bracket. Absent on the public pages, so the same
 * boxes render with no handler and no cursor change.
 */
export type BracketEditing = {
    /** Seeds may change: a side dragged onto another side swaps the two. */
    canSwap: boolean
    /** A side of a ready match was clicked. The editor decides what it means. */
    onPick: (match: Match, entrant: string) => void
    onSwap: (from: string, to: string) => void
    /** A match whose next match is decided takes no click. */
    isFrozen: (match: Match) => boolean
}

type BracketViewProps = {
    bracket: Bracket
    entrants: Entrant[]
    /** Larger type for a screen across the room. */
    kiosk?: boolean
    editing?: BracketEditing
}

type SideProps = {
    match: Match
    entrant: Entrant | undefined
    /** Absent side: a bye in round 1, an undecided feeder later. */
    placeholder: "bye" | "tbd"
    won: boolean
    decided: boolean
    interactive: boolean
    draggable: boolean
    dragging: boolean
    editing?: BracketEditing
    onDragStart: (entrant: string) => void
    onDragEnd: () => void
}

const Side = (props: SideProps) => {
    const entrant = props.entrant
    if (!entrant) {
        return (
            <div className="text-mute flex items-center gap-2 px-3 py-1.5 italic">
                {props.placeholder}
            </div>
        )
    }
    const tone = props.won
        ? "text-accent"
        : props.decided
          ? "text-mute line-through"
          : "text-ink"
    // In an editor a side that cannot take a click says why on hover.
    const waiting = Boolean(props.editing) && !props.interactive && !props.draggable
    const cursor = props.draggable
        ? "cursor-grab"
        : props.interactive
          ? "cursor-pointer"
          : waiting
            ? "cursor-not-allowed"
            : ""
    const hover = props.interactive || props.draggable ? "hover:bg-faint" : ""
    const faded = props.dragging ? "opacity-40" : ""
    const pick = () => props.editing?.onPick(props.match, entrant.id)

    return (
        <div
            className={`flex items-center gap-2 px-3 py-1.5 ${tone} ${cursor} ${hover} ${faded}`}
            data-entrant={entrant.id}
            role={props.interactive ? "button" : undefined}
            tabIndex={props.interactive ? 0 : undefined}
            aria-label={props.interactive ? `${entrantName(entrant)} wins` : undefined}
            title={
                waiting
                    ? props.decided
                        ? "the next match is decided. clear that result first"
                        : "waiting for the other side"
                    : undefined
            }
            onClick={props.interactive ? pick : undefined}
            onKeyDown={
                props.interactive
                    ? event => {
                          if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault()
                              pick()
                          }
                      }
                    : undefined
            }
            draggable={props.draggable}
            onDragStart={
                props.draggable
                    ? event => {
                          event.dataTransfer.setData("text/plain", entrant.id)
                          event.dataTransfer.effectAllowed = "move"
                          props.onDragStart(entrant.id)
                      }
                    : undefined
            }
            onDragEnd={props.draggable ? props.onDragEnd : undefined}
            onDragOver={props.draggable ? event => event.preventDefault() : undefined}
            onDrop={
                props.draggable
                    ? event => {
                          event.preventDefault()
                          const from = event.dataTransfer.getData("text/plain")
                          if (from && from !== entrant.id)
                              props.editing?.onSwap(from, entrant.id)
                          props.onDragEnd()
                      }
                    : undefined
            }
        >
            {entrant.player && <GlyphMark glyph={entrant.player.glyph} size="1.1em" />}
            <span className="truncate">{entrantName(entrant)}</span>
            {props.won && <span className="text-2xs ml-auto tracking-wider">win</span>}
        </div>
    )
}

/**
 * Every round as a column. The winner of a match is lit, the loser fades. With
 * `editing`, a side of a ready match takes a click, and round one sides can be
 * dragged onto each other to swap seeds.
 */
export const BracketView = (props: BracketViewProps) => {
    const [dragging, setDragging] = useState<string | undefined>()
    const byId = new Map(props.entrants.map(e => [e.id, e]))
    const rounds = props.bracket.rounds
    const final = rounds.at(-1)?.[0]
    const champion = final?.winner ? byId.get(final.winner) : undefined
    const textSize = props.kiosk ? "text-base" : "text-xs"
    const editing = props.editing

    const box = (m: Match) => {
        const decided = m.winner !== null && m.winner !== undefined
        const ready = Boolean(m.entrantA && m.entrantB)
        // In round 1 an empty side is a bye. Later an empty side waits for a result.
        const placeholder = m.round === 1 ? "bye" : "tbd"
        const interactive = Boolean(editing) && ready && !(editing?.isFrozen(m) ?? false)
        const draggable = Boolean(editing?.canSwap) && m.round === 1
        const side = (entrantId: string | null | undefined) => (
            <Side
                match={m}
                entrant={entrantId ? byId.get(entrantId) : undefined}
                placeholder={placeholder}
                won={decided && ready && m.winner === entrantId}
                decided={decided && ready}
                interactive={interactive}
                draggable={draggable}
                dragging={dragging !== undefined && dragging === entrantId}
                editing={editing}
                onDragStart={setDragging}
                onDragEnd={() => setDragging(undefined)}
            />
        )
        return (
            <div
                key={m.id}
                className={`border-border bg-panel border ${textSize}`}
                data-match={m.id}
            >
                {side(m.entrantA)}
                <div className="border-faint border-t" />
                {side(m.entrantB)}
            </div>
        )
    }

    return (
        <div className="overflow-x-auto pb-2">
            <div
                className="grid gap-6"
                style={{
                    gridTemplateColumns: `repeat(${rounds.length + 1}, minmax(${props.kiosk ? "16rem" : "12rem"}, 1fr))`,
                }}
            >
                {rounds.map((round, index) => (
                    <div key={index} className="flex flex-col gap-3">
                        <div className="text-2xs text-dim tracking-wider">
                            {roundLabel(index, rounds.length)}
                        </div>
                        <div className="flex flex-1 flex-col justify-around gap-3">
                            {round.map(box)}
                        </div>
                    </div>
                ))}
                <div className="flex flex-col gap-3">
                    <div className="text-2xs text-dim tracking-wider">champion</div>
                    <div
                        className={`flex flex-1 flex-col justify-center border p-4 text-center ${champion ? "border-accent" : "border-faint"}`}
                        data-champion={champion?.id}
                    >
                        {champion?.player ? (
                            <div className="flex flex-col items-center gap-3">
                                <GlyphMark
                                    glyph={champion.player.glyph}
                                    size={props.kiosk ? "6rem" : "4rem"}
                                    framed
                                />
                                <span
                                    className={`${props.kiosk ? "text-2xl" : "text-base"} text-accent tracking-wider`}
                                >
                                    {champion.player.handle}
                                </span>
                            </div>
                        ) : (
                            <span className={`${textSize} text-mute italic`}>
                                {champion ? entrantName(champion) : "tbd"}
                            </span>
                        )}
                    </div>
                </div>
            </div>
        </div>
    )
}

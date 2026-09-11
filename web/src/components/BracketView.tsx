import { useState } from "react"
import type { DragEvent } from "react"

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

const SIDE_BASE = "flex items-center gap-2 px-3 py-1.5"

/** Why a side of an editable bracket takes no gesture right now. */
const waitingTitle = (props: SideProps): string | undefined => {
    if (!props.editing || props.interactive || props.draggable) return undefined
    return props.decided
        ? "the next match is decided. clear that result first"
        : "waiting for the other side"
}

const toneOf = (props: SideProps): string => {
    if (props.won) return "text-accent"
    return props.decided ? "text-mute line-through" : "text-ink"
}

const Side = (props: SideProps) => {
    const entrant = props.entrant
    if (!entrant) {
        return <div className={`${SIDE_BASE} text-mute italic`}>{props.placeholder}</div>
    }

    const name = entrantName(entrant)
    const body = (
        <>
            {entrant.player && <GlyphMark glyph={entrant.player.glyph} size="1.1em" />}
            <span className="truncate">{name}</span>
            {props.won && <span className="text-2xs ml-auto tracking-wider">win</span>}
        </>
    )
    const tone = toneOf(props)

    // A side that takes no gesture carries no role and stays out of the tab
    // order. The public bracket renders only this arm.
    if (!props.interactive && !props.draggable) {
        const waiting = waitingTitle(props)
        return (
            <div
                className={`${SIDE_BASE} ${tone} ${waiting ? "cursor-not-allowed" : ""}`}
                data-entrant={entrant.id}
                title={waiting}
            >
                {body}
            </div>
        )
    }

    const faded = props.dragging ? "opacity-40" : ""

    // Only a round one side can be dragged, and only a round one side can be a
    // drop target. Without this guard every later side accepts a drop and swaps
    // seeds on a gesture nothing on screen offers.
    const dragProps = props.draggable
        ? {
              draggable: true,
              onDragStart: (event: DragEvent<HTMLElement>) => {
                  event.dataTransfer.setData("text/plain", entrant.id)
                  event.dataTransfer.effectAllowed = "move"
                  props.onDragStart(entrant.id)
              },
              onDragEnd: props.onDragEnd,
              onDragOver: (event: DragEvent<HTMLElement>) => event.preventDefault(),
              onDrop: (event: DragEvent<HTMLElement>) => {
                  event.preventDefault()
                  const from = event.dataTransfer.getData("text/plain")
                  if (from && from !== entrant.id) props.editing?.onSwap(from, entrant.id)
                  props.onDragEnd()
              },
          }
        : {}

    // A side that takes a result is a real button, so Enter and Space work with
    // no handler of our own. It can be dragged as well.
    if (props.interactive) {
        return (
            <button
                type="button"
                className={`${SIDE_BASE} w-full ${tone} ${faded} hover:bg-faint cursor-pointer`}
                data-entrant={entrant.id}
                data-pick=""
                aria-label={`${name} wins`}
                onClick={() => props.editing?.onPick(props.match, entrant.id)}
                {...dragProps}
            >
                {body}
            </button>
        )
    }

    // A side that can only be dragged is not a button: a keyboard cannot work
    // it, so it must not announce itself as one or take a tab stop.
    return (
        <div
            className={`${SIDE_BASE} ${tone} ${faded} hover:bg-faint cursor-grab`}
            data-entrant={entrant.id}
            aria-label={`${name}, drag onto another player to swap seeds`}
            title="drag onto another player to swap their seeds"
            {...dragProps}
        >
            {body}
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
                    <div
                        key={roundLabel(index, rounds.length)}
                        data-round={index + 1}
                        className="flex flex-col gap-3"
                    >
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

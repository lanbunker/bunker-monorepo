import { actions } from "astro:actions"
import { useEffect, useState } from "react"

import type { Bracket, Entrant, Match } from "../lib/api"
import { BracketView } from "./BracketView"
import type { BracketEditing } from "./BracketView"

type BracketEditorProps = {
    tournamentId: string
    entrants: Entrant[]
    bracket: Bracket | null | undefined
    /** A concluded tournament shows its bracket and takes no change. */
    locked: boolean
}

type ActionResult =
    | Awaited<ReturnType<typeof actions.reportResult>>
    | Awaited<ReturnType<typeof actions.deleteBracket>>

const button =
    "inline-flex cursor-pointer items-center gap-1 whitespace-nowrap border px-2 py-0.5 text-2xs font-bold tracking-wide disabled:cursor-not-allowed disabled:opacity-30"
const warn = `${button} border-warn/60 text-warn hover:bg-warn hover:text-black`
const danger = `${button} border-alert/60 text-alert hover:bg-alert hover:text-black`

const isPlayed = (m: Match) => Boolean(m.entrantA && m.entrantB && m.winner)

/**
 * The backoffice bracket: the public view with hands on it. A click on a side
 * enters that result, a click on the lit winner clears it, a round one side
 * dragged onto another swaps their seeds. Every gesture is one Astro action,
 * and the answer of the API replaces the bracket on screen, so the screen shows
 * what the database holds. Nothing is computed in the browser.
 */
export const BracketEditor = (props: BracketEditorProps) => {
    const [bracket, setBracket] = useState(props.bracket ?? null)
    // Seeds change with a swap, and the API answers the bracket only, so the
    // seeds are kept here and set from the order that was sent.
    const [entrants, setEntrants] = useState(props.entrants)
    const [error, setError] = useState<string | undefined>()
    const [busy, setBusy] = useState(false)
    // The controls work only after hydration. Until then they stay disabled, so
    // a click on the server-rendered markup cannot get lost.
    const [mounted, setMounted] = useState(false)
    useEffect(() => setMounted(true), [])
    const idle = mounted && !busy
    const id = props.tournamentId
    const hasResults = bracket?.rounds.flat().some(isPlayed) ?? false
    const canSwap = idle && !props.locked && !hasResults

    const run = async (call: () => Promise<ActionResult>) => {
        setBusy(true)
        setError(undefined)
        const result = await call()
        setBusy(false)
        if (result.error) {
            setError(result.error.message)
            return false
        }
        if (result.data) setBracket("deleted" in result.data ? null : result.data)
        return true
    }

    // Whether a bracket exists decides what the rest of the page shows: the
    // status buttons, the entrant controls, the kiosk link. Those parts are
    // server rendered, so a reload is the honest way to refresh them.
    const generate = () =>
        run(() => actions.generateBracket({ id })).then(
            ok => ok && window.location.reload(),
        )
    const remove = () => {
        if (window.confirm("remove the bracket? seeds are lost.")) {
            run(() => actions.deleteBracket({ id })).then(
                ok => ok && window.location.reload(),
            )
        }
    }

    /** The next match of `m`, which freezes `m` once it has a result. */
    const nextOf = (m: Match) => bracket?.rounds[m.round]?.[Math.floor(m.slot / 2)]

    const editing: BracketEditing = {
        canSwap,
        isFrozen: m => !idle || props.locked || Boolean(nextOf(m)?.winner),
        onPick: (m, entrant) => {
            if (m.winner === entrant) {
                run(() => actions.clearResult({ id, matchId: m.id }))
            } else {
                run(() => actions.reportResult({ id, matchId: m.id, winner: entrant }))
            }
        },
        onSwap: (from, to) => {
            // The two dragged entrants trade seeds. Everyone else keeps theirs.
            const order = [...entrants]
                .sort((a, b) => (a.seed ?? 0) - (b.seed ?? 0))
                .map(e => e.id)
            const swapped = order.map(e => (e === from ? to : e === to ? from : e))
            run(() => actions.reorderSeeds({ id, entrants: swapped })).then(ok => {
                if (ok) {
                    setEntrants(current =>
                        current.map(e => ({ ...e, seed: swapped.indexOf(e.id) + 1 })),
                    )
                }
            })
        },
    }

    return (
        <div className="flex flex-col gap-4 text-xs">
            {!props.locked && (
                <div className="flex flex-wrap items-center gap-2">
                    <button
                        type="button"
                        className={warn}
                        disabled={!idle || hasResults || entrants.length < 2}
                        onClick={generate}
                    >
                        {bracket ? "regenerate" : "generate bracket"}
                    </button>
                    {bracket && (
                        <button
                            type="button"
                            className={danger}
                            disabled={!idle || hasResults}
                            onClick={remove}
                        >
                            remove bracket
                        </button>
                    )}
                    {bracket && (
                        <span className="text-2xs text-admin-dim">
                            {hasResults
                                ? "click a side to set the winner, click the winner to clear it. clear every result to regenerate or reorder."
                                : "drag a player onto another to swap their seeds. click a side to set the winner."}
                        </span>
                    )}
                    {!bracket && entrants.length < 2 && (
                        <span className="text-2xs text-admin-dim">
                            two entrants at least.
                        </span>
                    )}
                </div>
            )}
            {error && (
                <p className="text-alert m-0 text-xs" role="alert">
                    {error}
                </p>
            )}
            {bracket && (
                <div className={busy ? "opacity-60" : ""} aria-busy={busy}>
                    <BracketView
                        bracket={bracket}
                        entrants={entrants}
                        editing={editing}
                    />
                </div>
            )}
        </div>
    )
}

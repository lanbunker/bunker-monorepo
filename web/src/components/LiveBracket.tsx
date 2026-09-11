import { useEffect, useState } from "react"

import type { TournamentDetail } from "../lib/api"
import { parseTournamentDetail } from "../lib/detail"
import { BracketView } from "./BracketView"

type LiveBracketProps = {
    initial: TournamentDetail
    /** The site route that answers the detail as JSON. */
    source: string
    intervalMs?: number
}

const DEFAULT_INTERVAL_MS = 3000

/**
 * The kiosk view. It starts from the server-rendered detail, then asks the
 * site for a fresh copy on a timer and swaps the state. A concluded tournament
 * stops the timer: nothing changes any more. A body that does not parse leaves
 * the last good state on screen, so a screen across the room never goes blank.
 */
export const LiveBracket = (props: LiveBracketProps) => {
    const [detail, setDetail] = useState(props.initial)
    const [offline, setOffline] = useState(false)
    const concluded = detail.tournament.status === "concluded"
    const intervalMs = props.intervalMs ?? DEFAULT_INTERVAL_MS
    const source = props.source

    useEffect(() => {
        if (concluded) return
        const poll = async () => {
            const fresh = await fetch(source, { cache: "no-store" })
                .then(response => (response.ok ? response.json() : undefined))
                .then(parseTournamentDetail)
                .catch(() => undefined)
            if (fresh) setDetail(fresh)
            setOffline(fresh === undefined)
        }
        const timer = window.setInterval(poll, intervalMs)
        return () => window.clearInterval(timer)
    }, [source, intervalMs, concluded])

    const t = detail.tournament
    const state = concluded
        ? { tone: "text-accent", label: "■ concluded" }
        : offline
          ? { tone: "text-alert", label: "■ site offline, showing the last state" }
          : { tone: "text-accent-soft", label: "■ live" }

    return (
        <div className="flex flex-col gap-6">
            <div className="flex flex-wrap items-baseline justify-between gap-4">
                <div>
                    <div className="text-2xs text-dim tracking-wider">{t.game}</div>
                    <h1 className="text-bright m-0 text-2xl tracking-wider">{t.name}</h1>
                    <div className="text-dim mt-1 text-xs">{t.mode}</div>
                </div>
                <div className="text-right text-xs">
                    <div className={state.tone}>{state.label}</div>
                    <div className="text-2xs text-mute mt-1">
                        {t.entrantCount} entrants
                    </div>
                </div>
            </div>
            {detail.bracket ? (
                <BracketView bracket={detail.bracket} entrants={detail.entrants} kiosk />
            ) : (
                <p className="text-dim text-base">the bracket is not generated yet.</p>
            )}
        </div>
    )
}

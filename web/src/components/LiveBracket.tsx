import { useEffect, useState } from "react"

import type { TournamentDetail } from "../lib/api"
import { BracketView } from "./BracketView"

type LiveBracketProps = {
    initial: TournamentDetail
    /** The site route that answers the detail as JSON. */
    source: string
    intervalMs?: number
}

/**
 * The kiosk view. It starts from the server-rendered detail, then asks the
 * site for a fresh copy on a timer and swaps the state. A concluded tournament
 * stops the timer: nothing changes any more.
 */
export const LiveBracket = (props: LiveBracketProps) => {
    const [detail, setDetail] = useState(props.initial)
    const [offline, setOffline] = useState(false)
    const concluded = detail.tournament.status === "concluded"

    useEffect(() => {
        if (concluded) return
        const timer = window.setInterval(async () => {
            try {
                const response = await fetch(props.source, { cache: "no-store" })
                if (!response.ok) throw new Error(String(response.status))
                const fresh: TournamentDetail = await response.json()
                setDetail(fresh)
                setOffline(false)
            } catch {
                setOffline(true)
            }
        }, props.intervalMs ?? 3000)
        return () => window.clearInterval(timer)
    }, [props.source, props.intervalMs, concluded])

    const t = detail.tournament
    return (
        <div className="flex flex-col gap-6">
            <div className="flex flex-wrap items-baseline justify-between gap-4">
                <div>
                    <div className="text-2xs text-dim tracking-wider">{t.game}</div>
                    <h1 className="text-bright m-0 text-2xl tracking-wider">{t.name}</h1>
                    <div className="text-dim mt-1 text-xs">{t.mode}</div>
                </div>
                <div className="text-right text-xs">
                    <div
                        className={
                            concluded
                                ? "text-accent"
                                : offline
                                  ? "text-alert"
                                  : "text-accent-soft"
                        }
                    >
                        {concluded
                            ? "■ concluded"
                            : offline
                              ? "■ site offline, showing the last state"
                              : "■ live"}
                    </div>
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

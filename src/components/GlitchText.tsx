import { useState, useEffect } from "react"

export function GlitchText({ children }: { children: React.ReactNode }) {
    const [glitch, setGlitch] = useState(false)
    const [offset, setOffset] = useState({ x: 0, y: 0, top: 0, bottom: 0 })

    useEffect(() => {
        const iv = setInterval(() => {
            if (Math.random() < 0.08) {
                setOffset({
                    x: Math.random() * 4 - 2,
                    y: Math.random() * 4 - 2,
                    top: Math.random() * 40,
                    bottom: Math.random() * 40,
                })
                setGlitch(true)
                setTimeout(() => setGlitch(false), 80 + Math.random() * 120)
            }
        }, 2000)
        return () => clearInterval(iv)
    }, [])

    return (
        <span className="relative inline-block">
            <span className={glitch ? "opacity-0" : "opacity-100"}>{children}</span>
            {glitch && (
                <span
                    className="text-accent absolute"
                    style={{
                        left: `${offset.x}px`,
                        top: `${offset.y}px`,
                        clipPath: `inset(${offset.top}% 0 ${offset.bottom}% 0)`,
                    }}
                >
                    {children}
                </span>
            )}
        </span>
    )
}

// Ported from https://reactbits.dev/text-animations/ascii-text
// Original: https://codepen.io/JuanFuentes/pen/eYEeoyE

import { useEffect, useRef } from "react"
import * as THREE from "three"

const vertexShader = `
varying vec2 vUv;
uniform float uTime;
uniform float uEnableWaves;

void main() {
    vUv = uv;
    float time = uTime * 5.;
    float waveFactor = uEnableWaves;
    vec3 transformed = position;
    transformed.x += sin(time + position.y) * 0.5 * waveFactor;
    transformed.y += cos(time + position.z) * 0.15 * waveFactor;
    transformed.z += sin(time + position.x) * waveFactor;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(transformed, 1.0);
}
`

const fragmentShader = `
varying vec2 vUv;
uniform float uTime;
uniform sampler2D uTexture;

void main() {
    float time = uTime;
    vec2 pos = vUv;
    float r = texture2D(uTexture, pos + cos(time * 2. - time + pos.x) * .01).r;
    float g = texture2D(uTexture, pos + tan(time * .5 + pos.x - time) * .01).g;
    float b = texture2D(uTexture, pos - cos(time * 2. + time + pos.y) * .01).b;
    float a = texture2D(uTexture, pos).a;
    gl_FragColor = vec4(r, g, b, a);
}
`

function mapRange(n: number, start: number, stop: number, start2: number, stop2: number) {
    return ((n - start) / (stop - start)) * (stop2 - start2) + start2
}

const PX_RATIO = typeof window !== "undefined" ? window.devicePixelRatio : 1

class AsciiFilter {
    domElement: HTMLDivElement
    pre: HTMLPreElement
    canvas: HTMLCanvasElement
    context: CanvasRenderingContext2D
    renderer: THREE.WebGLRenderer
    deg = 0
    invert: boolean
    fontSize: number
    fontFamily: string
    charset: string
    cols = 0
    rows = 0
    width = 0
    height = 0
    center = { x: 0, y: 0 }
    mouse = { x: 0, y: 0 }

    constructor(
        renderer: THREE.WebGLRenderer,
        {
            fontSize = 12,
            fontFamily = "'Courier New', monospace",
            charset,
            invert = true,
        }: {
            fontSize?: number
            fontFamily?: string
            charset?: string
            invert?: boolean
        } = {},
    ) {
        this.renderer = renderer
        this.domElement = document.createElement("div")
        this.domElement.style.position = "absolute"
        this.domElement.style.top = "0"
        this.domElement.style.left = "0"
        this.domElement.style.width = "100%"
        this.domElement.style.height = "100%"

        this.pre = document.createElement("pre")
        this.domElement.appendChild(this.pre)

        this.canvas = document.createElement("canvas")
        this.context = this.canvas.getContext("2d")!
        this.domElement.appendChild(this.canvas)

        this.invert = invert
        this.fontSize = fontSize
        this.fontFamily = fontFamily
        this.charset =
            charset ??
            " .'`^\",:;Il!i~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"

        this.onMouseMove = this.onMouseMove.bind(this)
        document.addEventListener("mousemove", this.onMouseMove)
    }

    setSize(width: number, height: number) {
        this.width = width
        this.height = height
        this.renderer.setSize(width, height)
        this.reset()
        this.center = { x: width / 2, y: height / 2 }
        this.mouse = { x: this.center.x, y: this.center.y }
    }

    reset() {
        this.context.font = `${this.fontSize}px ${this.fontFamily}`
        const charWidth = this.context.measureText("A").width
        this.cols = Math.floor(this.width / (this.fontSize * (charWidth / this.fontSize)))
        this.rows = Math.floor(this.height / this.fontSize)
        this.canvas.width = this.cols
        this.canvas.height = this.rows
        this.pre.style.fontFamily = this.fontFamily
        this.pre.style.fontSize = `${this.fontSize}px`
        this.pre.style.margin = "0"
        this.pre.style.padding = "0"
        this.pre.style.lineHeight = "1em"
        this.pre.style.position = "absolute"
        this.pre.style.left = "0"
        this.pre.style.top = "0"
        this.pre.style.zIndex = "9"
        this.pre.style.backgroundAttachment = "fixed"
        this.pre.style.mixBlendMode = "difference"
    }

    render(scene: THREE.Scene, camera: THREE.Camera) {
        this.renderer.render(scene, camera)
        const w = this.canvas.width
        const h = this.canvas.height
        this.context.clearRect(0, 0, w, h)
        if (w && h) {
            this.context.drawImage(this.renderer.domElement, 0, 0, w, h)
        }
        this.asciify(w, h)
        this.hue()
    }

    onMouseMove(e: MouseEvent) {
        this.mouse = { x: e.clientX * PX_RATIO, y: e.clientY * PX_RATIO }
    }

    hue() {
        const dx = this.mouse.x - this.center.x
        const dy = this.mouse.y - this.center.y
        const deg = (Math.atan2(dy, dx) * 180) / Math.PI
        this.deg += (deg - this.deg) * 0.075
        this.domElement.style.filter = `hue-rotate(${this.deg.toFixed(1)}deg)`
    }

    asciify(w: number, h: number) {
        if (!w || !h) return
        const imgData = this.context.getImageData(0, 0, w, h).data
        let str = ""
        for (let y = 0; y < h; y++) {
            for (let x = 0; x < w; x++) {
                const i = (x + y * w) * 4
                const [r, g, b, a] = [
                    imgData[i],
                    imgData[i + 1],
                    imgData[i + 2],
                    imgData[i + 3],
                ]
                if (a === 0) {
                    str += " "
                    continue
                }
                const gray = (0.3 * r + 0.6 * g + 0.1 * b) / 255
                let idx = Math.floor((1 - gray) * (this.charset.length - 1))
                if (this.invert) idx = this.charset.length - idx - 1
                str += this.charset[idx]
            }
            str += "\n"
        }
        this.pre.textContent = str
    }

    dispose() {
        document.removeEventListener("mousemove", this.onMouseMove)
    }
}

class CanvasTxt {
    canvas: HTMLCanvasElement
    context: CanvasRenderingContext2D
    txt: string
    fontSize: number
    font: string
    color: string

    constructor(
        txt: string,
        { fontSize = 200, fontFamily = "Arial", color = "#fdf9f3" } = {},
    ) {
        this.canvas = document.createElement("canvas")
        this.context = this.canvas.getContext("2d")!
        this.txt = txt
        this.fontSize = fontSize
        this.color = color
        this.font = `600 ${this.fontSize}px ${fontFamily}`
    }

    resize() {
        this.context.font = this.font
        const metrics = this.context.measureText(this.txt)
        this.canvas.width = Math.ceil(metrics.width) + 20
        this.canvas.height =
            Math.ceil(
                metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent,
            ) + 20
    }

    render() {
        this.context.clearRect(0, 0, this.canvas.width, this.canvas.height)
        this.context.fillStyle = this.color
        this.context.font = this.font
        const metrics = this.context.measureText(this.txt)
        this.context.fillText(this.txt, 10, 10 + metrics.actualBoundingBoxAscent)
    }

    get texture() {
        return this.canvas
    }
}

interface Props {
    text?: string
    asciiFontSize?: number
    textFontSize?: number
    textColor?: string
    planeBaseHeight?: number
    enableWaves?: boolean
    fallback?: string
}

function hasWebGL(): boolean {
    try {
        const canvas = document.createElement("canvas")
        return !!(canvas.getContext("webgl") || canvas.getContext("experimental-webgl"))
    } catch {
        return false
    }
}

export default function ASCIIText({
    text = "BUNKER",
    asciiFontSize = 12,
    textFontSize = 200,
    textColor = "#00ff41",
    planeBaseHeight = 8,
    enableWaves = true,
    fallback,
}: Props) {
    const containerRef = useRef<HTMLDivElement>(null)
    const failedRef = useRef(false)
    const asciiRef = useRef<{
        load: () => void
        setSize: (w: number, h: number) => void
        dispose: () => void
    } | null>(null)

    if (!hasWebGL()) {
        return (
            <pre
                style={{
                    color: "#fff",
                    fontSize: "8.5px",
                    lineHeight: "1.15",
                    whiteSpace: "pre",
                    margin: 0,
                    display: "inline-block",
                    minWidth: "max-content",
                    fontFamily: "'Courier New', 'Lucida Console', monospace",
                }}
            >
                {fallback ?? text}
            </pre>
        )
    }

    useEffect(() => {
        if (!containerRef.current) return
        let cancelled = false
        let observer: IntersectionObserver | null = null
        let ro: ResizeObserver | null = null

        const createInstance = async (container: HTMLElement, w: number, h: number) => {
            const camera = new THREE.PerspectiveCamera(45, w / h, 1, 1000)
            camera.position.z = 30
            const scene = new THREE.Scene()
            let mouse = { x: w / 2, y: h / 2 }

            const textCanvas = new CanvasTxt(text, {
                fontSize: textFontSize,
                fontFamily: "IBM Plex Mono",
                color: textColor,
            })
            textCanvas.resize()
            textCanvas.render()

            const texture = new THREE.CanvasTexture(textCanvas.texture)
            texture.minFilter = THREE.NearestFilter

            const textAspect = textCanvas.canvas.width / textCanvas.canvas.height
            const geometry = new THREE.PlaneGeometry(
                planeBaseHeight * textAspect,
                planeBaseHeight,
                36,
                36,
            )
            const material = new THREE.ShaderMaterial({
                vertexShader,
                fragmentShader,
                transparent: true,
                uniforms: {
                    uTime: { value: 0 },
                    mouse: { value: 1.0 },
                    uTexture: { value: texture },
                    uEnableWaves: { value: enableWaves ? 1.0 : 0.0 },
                },
            })
            const mesh = new THREE.Mesh(geometry, material)
            scene.add(mesh)

            const renderer = new THREE.WebGLRenderer({
                antialias: false,
                alpha: true,
            })
            renderer.setPixelRatio(1)
            renderer.setClearColor(0x000000, 0)

            const filter = new AsciiFilter(renderer, {
                fontFamily: "IBM Plex Mono",
                fontSize: asciiFontSize,
                invert: true,
            })
            container.appendChild(filter.domElement)
            filter.setSize(w, h)

            const onMouseMove = (evt: MouseEvent | TouchEvent) => {
                const e = "touches" in evt ? evt.touches[0] : evt
                const bounds = container.getBoundingClientRect()
                mouse = { x: e.clientX - bounds.left, y: e.clientY - bounds.top }
            }
            container.addEventListener("mousemove", onMouseMove)
            container.addEventListener("touchmove", onMouseMove)

            let frameId = 0
            return {
                load() {
                    const animate = () => {
                        frameId = requestAnimationFrame(animate)
                        const time = Date.now() * 0.001
                        textCanvas.render()
                        texture.needsUpdate = true
                        material.uniforms.uTime.value = Math.sin(time)
                        mesh.rotation.x +=
                            (mapRange(mouse.y, 0, h, 0.5, -0.5) - mesh.rotation.x) * 0.05
                        mesh.rotation.y +=
                            (mapRange(mouse.x, 0, w, -0.5, 0.5) - mesh.rotation.y) * 0.05
                        filter.render(scene, camera)
                    }
                    animate()
                },
                setSize(nw: number, nh: number) {
                    w = nw
                    h = nh
                    camera.aspect = nw / nh
                    camera.updateProjectionMatrix()
                    filter.setSize(nw, nh)
                },
                dispose() {
                    cancelAnimationFrame(frameId)
                    filter.dispose()
                    if (filter.domElement.parentNode)
                        container.removeChild(filter.domElement)
                    container.removeEventListener("mousemove", onMouseMove)
                    container.removeEventListener("touchmove", onMouseMove)
                    scene.traverse(obj => {
                        if (obj instanceof THREE.Mesh) {
                            obj.geometry.dispose()
                            if (obj.material instanceof THREE.Material)
                                obj.material.dispose()
                        }
                    })
                    scene.clear()
                    renderer.dispose()
                    renderer.forceContextLoss()
                },
            }
        }

        const showFallback = () => {
            if (!containerRef.current || failedRef.current) return
            failedRef.current = true
            const pre = document.createElement("pre")
            pre.textContent = fallback ?? text
            pre.style.cssText =
                "color:#fff;font-size:8.5px;line-height:1.15;white-space:pre;margin:0;font-family:'Courier New','Lucida Console',monospace"
            containerRef.current.appendChild(pre)
        }

        const setup = async () => {
            const { width, height } = containerRef.current!.getBoundingClientRect()
            if (width === 0 || height === 0) {
                observer = new IntersectionObserver(
                    async ([entry]) => {
                        if (cancelled || !entry.isIntersecting) return
                        const { width: w, height: h } = entry.boundingClientRect
                        if (w > 0 && h > 0) {
                            observer?.disconnect()
                            observer = null
                            if (!cancelled) {
                                asciiRef.current = await createInstance(
                                    containerRef.current!,
                                    w,
                                    h,
                                )
                                if (!cancelled) asciiRef.current.load()
                            }
                        }
                    },
                    { threshold: 0.1 },
                )
                observer.observe(containerRef.current!)
                return
            }

            asciiRef.current = await createInstance(containerRef.current!, width, height)
            if (!cancelled) {
                asciiRef.current.load()
                ro = new ResizeObserver(entries => {
                    const rect = entries[0]?.contentRect
                    if (rect && rect.width > 0 && rect.height > 0) {
                        asciiRef.current?.setSize(rect.width, rect.height)
                    }
                })
                ro.observe(containerRef.current!)
            }
        }

        setup().catch(() => showFallback())
        return () => {
            cancelled = true
            observer?.disconnect()
            ro?.disconnect()
            asciiRef.current?.dispose()
            asciiRef.current = null
        }
    }, [text, asciiFontSize, textFontSize, textColor, planeBaseHeight, enableWaves])

    return (
        <>
            <link
                href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@500;600&display=swap"
                rel="stylesheet"
            />
            <div
                ref={containerRef}
                className="ascii-text-container"
                style={{ position: "relative", width: "100%", height: "100%" }}
            >
                <style>{`
                    .ascii-text-container canvas {
                        position: absolute;
                        left: 0;
                        top: 0;
                        width: 100%;
                        height: 100%;
                        image-rendering: optimizeSpeed;
                        image-rendering: -moz-crisp-edges;
                        image-rendering: -webkit-optimize-contrast;
                        image-rendering: crisp-edges;
                        image-rendering: pixelated;
                    }
                    .ascii-text-container pre {
                        margin: 0;
                        user-select: none;
                        padding: 0;
                        line-height: 1em;
                        text-align: left;
                        position: absolute;
                        left: 0;
                        top: 0;
                        background-image: radial-gradient(circle, #ff6188 0%, #fc9867 50%, #ffd866 100%);
                        background-attachment: fixed;
                        -webkit-text-fill-color: transparent;
                        -webkit-background-clip: text;
                        background-clip: text;
                        z-index: 9;
                        mix-blend-mode: difference;
                    }
                `}</style>
            </div>
        </>
    )
}

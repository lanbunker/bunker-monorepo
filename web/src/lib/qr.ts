import makeQr from "qrcode-generator"

/** Modules per side of the quiet zone the QR standard asks for. */
const QUIET = 4
/** Pixels per module in the SVG. The viewer scales the image, so this only sets the ratio of the caption to the code. */
const CELL = 10

/**
 * The `Char` production of XML 1.0: tab, line feed, carriage return, and every
 * code point from U+0020 but the surrogates, U+FFFE and U+FFFF. A string splits
 * into code points, so a lone surrogate arrives here on its own.
 */
const isXmlText = (char: string): boolean => {
    const code = char.codePointAt(0) ?? 0
    return (
        code === 0x09 ||
        code === 0x0a ||
        code === 0x0d ||
        (code >= 0x20 && code <= 0xd7ff) ||
        (code >= 0xe000 && code <= 0xfffd) ||
        code >= 0x10000
    )
}

/** Text safe inside the SVG: markup escaped, and the control characters XML refuses removed. */
const escape = (text: string): string =>
    [...text]
        .filter(isXmlText)
        .join("")
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll('"', "&quot;")

/**
 * A poster for the door: the check-in link as a QR code, black on white so any
 * printer and any phone camera read it, with the event name and CHECK-IN
 * under it. One SVG, so it scales to a sticker or an A4 sheet.
 */
export const checkinPoster = (url: string, title: string): string => {
    // Type 0 picks the smallest version that fits. M corrects 15% of damage, enough
    // for a print on a door.
    const qr = makeQr(0, "M")
    qr.addData(url)
    qr.make()
    const count = qr.getModuleCount()
    const side = (count + QUIET * 2) * CELL

    const cells = Array.from({ length: count }, (_row, row) =>
        Array.from({ length: count }, (_col, col) =>
            qr.isDark(row, col)
                ? `M${(col + QUIET) * CELL} ${(row + QUIET) * CELL}h${CELL}v${CELL}h-${CELL}z`
                : "",
        ).join(""),
    ).join("")

    // A monospace glyph is about 0.6 em wide. A long name shrinks until it fits
    // the width of the code, instead of running off the poster.
    const fitted = (size: number, text: string) =>
        Math.min(size, (side - CELL * 2) / (Math.max(text.length, 1) * 0.6))
    const nameSize = fitted(CELL * 1.6, title)
    const urlSize = fitted(CELL * 1.1, url)

    const caption = CELL * 9
    const height = side + caption
    return [
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${side} ${height}" width="${side}" height="${height}">`,
        `<rect width="${side}" height="${height}" fill="#fff"/>`,
        `<path d="${cells}" fill="#000"/>`,
        `<text x="${side / 2}" y="${side + CELL * 2.5}" text-anchor="middle" font-family="monospace" font-weight="bold" font-size="${CELL * 3}" letter-spacing="${CELL * 0.4}" fill="#000">CHECK-IN</text>`,
        `<text x="${side / 2}" y="${side + CELL * 5.5}" text-anchor="middle" font-family="monospace" font-size="${nameSize}" fill="#000">${escape(title)}</text>`,
        `<text x="${side / 2}" y="${side + CELL * 7.5}" text-anchor="middle" font-family="monospace" font-size="${urlSize}" fill="#555">${escape(url)}</text>`,
        "</svg>",
    ].join("")
}

/**
 * A file name for the poster: the event name in lowercase, with each other run
 * of characters as one hyphen.
 */
export const posterFileName = (title: string, extension: "svg" | "png"): string => {
    const slug = title
        .toLowerCase()
        .replaceAll(/[^a-z0-9]+/g, "-")
        .replaceAll(/^-|-$/g, "")
    return `checkin-${slug || "event"}.${extension}`
}

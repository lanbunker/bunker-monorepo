import { readdir, stat, unlink } from "node:fs/promises"
import { extname, join, basename } from "node:path"
import sharp from "sharp"

const ASSETS_DIR = "src/assets/images"
const EXTENSIONS = new Set([".jpg", ".jpeg", ".png", ".tiff", ".bmp"])
const QUALITY = 85
const MAX_WIDTH = 2000

const processDir = async (dir) => {
  const entries = await readdir(dir, { withFileTypes: true })

  for (const entry of entries) {
    const fullPath = join(dir, entry.name)

    if (entry.isDirectory()) {
      await processDir(fullPath)
      continue
    }

    const ext = extname(entry.name).toLowerCase()
    if (!EXTENSIONS.has(ext)) continue

    const outPath = join(dir, basename(entry.name, extname(entry.name)) + ".webp")

    const img = sharp(fullPath)
    const meta = await img.metadata()
    const needsResize = meta.width && meta.width > MAX_WIDTH

    await img
      .resize(needsResize ? { width: MAX_WIDTH, withoutEnlargement: true } : undefined)
      .webp({ quality: QUALITY })
      .toFile(outPath)

    const origSize = (await stat(fullPath)).size
    const newSize = (await stat(outPath)).size
    const saved = origSize > 0 ? ((1 - newSize / origSize) * 100).toFixed(0) : "?"

    console.log(`${entry.name} -> ${basename(outPath)} (${formatBytes(origSize)} -> ${formatBytes(newSize)}, -${saved}%)`)

    await unlink(fullPath)
  }
}

const formatBytes = (bytes) => {
  if (bytes < 1024) return `${bytes}B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`
}

console.log(`Optimizing images in ${ASSETS_DIR}/ -> WebP (quality ${QUALITY}, max ${MAX_WIDTH}px)\n`)
await processDir(ASSETS_DIR)
console.log("\nDone.")

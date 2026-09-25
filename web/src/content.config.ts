import { glob } from "astro/loaders"
import { defineCollection } from "astro:content"
import { z } from "zod"

const about = defineCollection({
    loader: glob({ pattern: "about.md", base: "./src/content" }),
    schema: z.object({
        title: z.string(),
    }),
})

export const collections = { about }

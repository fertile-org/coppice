import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import postcss from 'postcss'
import tailwindcss from 'tailwindcss'
import type { Config } from 'tailwindcss'
import { describe, expect, it } from 'vitest'

const webRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')
const sampleClasses = 'border-accent/40 border-border/60 hover:bg-accent/15'

describe('tailwind CSS-variable opacity modifiers', () => {
  it('emits rules for border-accent/40, border-border/60, and hover:bg-accent/15', async () => {
    const { default: config } = (await import(
      pathToFileURL(path.join(webRoot, 'tailwind.config.js')).href
    )) as { default: Config }

    const result = await postcss([
      tailwindcss({
        ...config,
        content: [{ raw: sampleClasses, extension: 'html' }],
      }),
    ]).process('@tailwind utilities;', { from: undefined })

    expect(result.css).toMatch(/\.border-accent\\\/40\s*\{/)
    expect(result.css).toMatch(/\.border-border\\\/60\s*\{/)
    expect(result.css).toMatch(/\.hover\\:bg-accent\\\/15:hover\s*\{/)
    expect(result.css).toContain('color-mix(in srgb')
  })
})

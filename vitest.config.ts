import { defineConfig } from 'vitest/config'
import { fileURLToPath, URL } from 'node:url'

// Only the pure modules are tested here — matching, layout maths, path helpers.
// Component behaviour is covered by using the application, which is cheaper
// than maintaining a DOM harness for a single-window app.
export default defineConfig({
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
})

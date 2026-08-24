import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath, URL } from 'node:url'

// Tauri drives the dev server; it must be fixed-port and must not clear the screen.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: 'safari15',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // Vite 8 minifies with oxc; naming esbuild here would pull in a dep we do not need.
    /*
     * Not minified, deliberately.
     *
     * Vite 8's minifier produced a bundle that killed the WebKit content
     * process in a release build — a blank window, with the backend running
     * fine behind it. The same code unminified renders. Same experiment, one
     * variable.
     *
     * Minifying buys nothing here anyway: these assets are embedded in the
     * binary and never travel over a network. A few hundred kilobytes of disk
     * is not worth a build that does not open.
     */
    minify: false,
  },
})

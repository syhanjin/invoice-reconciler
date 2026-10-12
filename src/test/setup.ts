import { configure } from "@testing-library/react"
import "@testing-library/jest-dom/vitest"

// jsdom + Tailwind CSS rendering make the 1000 ms default too tight for shared
// CI runners: async queries wait for a mocked backend round trip plus a React
// commit, which regularly exceeded one second on a 4 vCPU runner and failed
// unrelated tests. Five seconds keeps failures meaningful while removing the
// machine speed flake.
configure({ asyncUtilTimeout: 5000 })

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    dispatchEvent: () => false,
  }),
})

import { createStore } from "jotai"
import { describe, expect, it } from "vitest"

import {
  activeBatchAtom,
  mergeBatchAtom,
  mergeBatchEventAtom,
  ordersRefreshAtom,
} from "./store"
import type { BatchDetail } from "./types"

function batch(id: string, succeeded: number): BatchDetail {
  return {
    id,
    platform: "taobao",
    status: "processing",
    createdAt: "2026-08-12T00:00:00Z",
    completedAt: null,
    counts: {
      total: 2,
      queued: 2 - succeeded,
      processing: 0,
      succeeded,
      failed: 0,
      canceled: 0,
      skipped: 0,
      interrupted: 0,
    },
    tasks: [],
  }
}

describe("batch state", () => {
  it("refreshes orders when the successful task count increases", () => {
    const store = createStore()
    store.set(mergeBatchAtom, batch("current", 0))
    store.set(mergeBatchEventAtom, batch("current", 1))

    expect(store.get(activeBatchAtom)?.counts.succeeded).toBe(1)
    expect(store.get(ordersRefreshAtom)).toBe(1)
  })

  it("ignores delayed events from a batch that is no longer active", () => {
    const store = createStore()
    store.set(mergeBatchAtom, batch("current", 0))
    store.set(mergeBatchEventAtom, batch("previous", 2))

    expect(store.get(activeBatchAtom)?.id).toBe("current")
    expect(store.get(ordersRefreshAtom)).toBe(0)
  })
})

import { createStore } from "jotai"
import { describe, expect, it } from "vitest"

import {
  activeBatchAtom,
  defaultOrderTablePreferences,
  mergeBatchAtom,
  mergeBatchEventAtom,
  normalizeOrderTablePreferences,
  orderTablePreferencesAtom,
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

describe("order table preferences", () => {
  it("normalizes persisted values and drops unknown table state", () => {
    expect(normalizeOrderTablePreferences({
      pageSize: 50,
      sortBy: "paid_amount",
      sortDirection: "asc",
      density: "compact",
      columnVisibility: {
        productSummary: false,
        externalOrderNumber: false,
        removedColumn: false,
      },
      columnSizing: {
        productSummary: 9999,
        paidAmountFen: 20,
        removedColumn: 200,
      },
    })).toEqual({
      pageSize: 50,
      sortBy: "paid_amount",
      sortDirection: "asc",
      density: "compact",
      columnVisibility: { productSummary: false },
      columnSizing: { productSummary: 520, paidAmountFen: 100 },
    })
  })

  it("falls back to defaults for malformed persisted values", () => {
    expect(normalizeOrderTablePreferences({
      pageSize: 25,
      sortBy: "unknown",
      sortDirection: "sideways",
      density: "dense",
      columnVisibility: "invalid",
      columnSizing: null,
    })).toEqual(defaultOrderTablePreferences)
  })

  it("restores preferences from versioned local storage in a new store", () => {
    localStorage.clear()
    const firstStore = createStore()
    firstStore.set(orderTablePreferencesAtom, {
      ...defaultOrderTablePreferences,
      pageSize: 100,
      density: "compact",
      columnVisibility: { platform: false },
    })

    const secondStore = createStore()
    const unsubscribe = secondStore.sub(orderTablePreferencesAtom, () => undefined)
    expect(secondStore.get(orderTablePreferencesAtom)).toEqual({
      ...defaultOrderTablePreferences,
      pageSize: 100,
      density: "compact",
      columnVisibility: { platform: false },
    })
    unsubscribe()
  })
})

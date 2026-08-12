import { createStore } from "jotai"
import { describe, expect, it } from "vitest"

import {
  defaultOrderTablePreferences,
  mergeOrderImportEventAtom,
  normalizeOrderTablePreferences,
  orderImportRefreshAtom,
  orderTablePreferencesAtom,
  ordersRefreshAtom,
} from "./store"

describe("order import pool events", () => {
  it("refreshes the pool and orders when a task completes", () => {
    const store = createStore()
    store.set(mergeOrderImportEventAtom, {
      kind: "task_completed",
      taskId: "task-1",
      orderId: "order-1",
      recognitionBatchId: null,
    })

    expect(store.get(orderImportRefreshAtom)).toBe(1)
    expect(store.get(ordersRefreshAtom)).toBe(1)
  })

  it("refreshes only the pool for non-completion events", () => {
    const store = createStore()
    store.set(mergeOrderImportEventAtom, {
      kind: "task_failed",
      taskId: "task-1",
      orderId: null,
      recognitionBatchId: null,
    })

    expect(store.get(orderImportRefreshAtom)).toBe(1)
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

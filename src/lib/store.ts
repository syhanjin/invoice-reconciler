import { atom } from "jotai"

import type { AppSettings, BatchDetail, NavigationView } from "@/lib/types"

export const navigationAtom = atom<NavigationView>("orders")
export const settingsAtom = atom<AppSettings | null>(null)
export const activeBatchAtom = atom<BatchDetail | null>(null)
export const selectedOrderIdAtom = atom<string | null>(null)
export const ordersRefreshAtom = atom(0)

export const mergeBatchAtom = atom(null, (get, set, batch: BatchDetail) => {
  const previousSucceeded = get(activeBatchAtom)?.counts.succeeded ?? 0
  set(activeBatchAtom, batch)
  if (batch.counts.succeeded > previousSucceeded) {
    set(ordersRefreshAtom, (value) => value + 1)
  }
})

export const mergeBatchEventAtom = atom(null, (get, set, batch: BatchDetail) => {
  const activeBatch = get(activeBatchAtom)
  if (activeBatch && activeBatch.id !== batch.id) return
  set(mergeBatchAtom, batch)
})

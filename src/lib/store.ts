import { atom } from "jotai"
import { atomWithStorage } from "jotai/utils"

import type {
  AppSettings,
  NavigationView,
  OrderSortBy,
  OrderTableColumnId,
  OrderTablePreferences,
  SortDirection,
  InvoiceBatchDetail,
} from "@/lib/types"

const pageSizes = new Set([20, 50, 100])
const sortFields = new Set<OrderSortBy>([
  "created_at",
  "ordered_at",
  "paid_amount",
  "shop_name",
])
const sortDirections = new Set<SortDirection>(["asc", "desc"])
const optionalColumnIds = new Set<OrderTableColumnId>([
  "productSummary",
  "shopName",
  "paidAmountFen",
  "orderedAt",
  "createdAt",
  "platform",
  "matchStatus",
  "projectName",
])
const columnWidthBounds: Partial<Record<OrderTableColumnId, [number, number]>> = {
  externalOrderNumber: [170, 320],
  productSummary: [220, 520],
  shopName: [120, 280],
  paidAmountFen: [100, 180],
  orderedAt: [150, 230],
  createdAt: [150, 230],
  platform: [80, 130],
  matchStatus: [90, 130],
  projectName: [120, 240],
}

export const defaultOrderTablePreferences: OrderTablePreferences = {
  pageSize: 20,
  sortBy: "created_at",
  sortDirection: "desc",
  columnVisibility: {},
  columnSizing: {},
  density: "comfortable",
}

export function normalizeOrderTablePreferences(value: unknown): OrderTablePreferences {
  if (!value || typeof value !== "object") return defaultOrderTablePreferences
  const candidate = value as Partial<OrderTablePreferences>
  const columnVisibility: OrderTablePreferences["columnVisibility"] = {}
  const columnSizing: OrderTablePreferences["columnSizing"] = {}

  if (candidate.columnVisibility && typeof candidate.columnVisibility === "object") {
    for (const [id, visible] of Object.entries(candidate.columnVisibility)) {
      if (optionalColumnIds.has(id as OrderTableColumnId) && typeof visible === "boolean") {
        columnVisibility[id as OrderTableColumnId] = visible
      }
    }
  }
  if (candidate.columnSizing && typeof candidate.columnSizing === "object") {
    for (const [id, width] of Object.entries(candidate.columnSizing)) {
      const bounds = columnWidthBounds[id as OrderTableColumnId]
      if (bounds && typeof width === "number" && Number.isFinite(width)) {
        columnSizing[id as OrderTableColumnId] = Math.min(bounds[1], Math.max(bounds[0], width))
      }
    }
  }

  return {
    pageSize: pageSizes.has(candidate.pageSize ?? 0) ? candidate.pageSize! : 20,
    sortBy: sortFields.has(candidate.sortBy as OrderSortBy)
      ? candidate.sortBy as OrderSortBy
      : "created_at",
    sortDirection: sortDirections.has(candidate.sortDirection as SortDirection)
      ? candidate.sortDirection as SortDirection
      : "desc",
    columnVisibility,
    columnSizing,
    density: candidate.density === "compact" ? "compact" : "comfortable",
  }
}

const storedOrderTablePreferencesAtom = atomWithStorage<unknown>(
  "invoice-reconciler.orders-table.preferences.v1",
  defaultOrderTablePreferences,
  undefined,
  { getOnInit: true },
)

export const orderTablePreferencesAtom = atom(
  (get) => normalizeOrderTablePreferences(get(storedOrderTablePreferencesAtom)),
  (get, set, update: OrderTablePreferences | ((value: OrderTablePreferences) => OrderTablePreferences)) => {
    const current = normalizeOrderTablePreferences(get(storedOrderTablePreferencesAtom))
    const next = typeof update === "function" ? update(current) : update
    set(storedOrderTablePreferencesAtom, normalizeOrderTablePreferences(next))
  },
)

export const navigationAtom = atom<NavigationView>("orders")
export const settingsAtom = atom<AppSettings | null>(null)
export const selectedOrderIdAtom = atom<string | null>(null)
export const ordersRefreshAtom = atom(0)
export const orderImportRefreshAtom = atom(0)
export const activeInvoiceBatchAtom = atom<InvoiceBatchDetail | null>(null)
export const invoicesRefreshAtom = atom(0)
export const projectsRefreshAtom = atom(0)
export const workspaceRefreshAtom = atom(0)

export const mergeOrderImportEventAtom = atom(
  null,
  (_get, set, event: import("@/lib/types").OrderImportUpdateEvent) => {
    set(orderImportRefreshAtom, (value) => value + 1)
    if (event.kind === "task_completed") {
      set(ordersRefreshAtom, (value) => value + 1)
    }
  },
)

export const mergeInvoiceBatchAtom = atom(null, (get, set, batch: InvoiceBatchDetail) => {
  const previousSucceeded = get(activeInvoiceBatchAtom)?.counts.succeeded ?? 0
  set(activeInvoiceBatchAtom, batch)
  if (batch.counts.succeeded > previousSucceeded) {
    set(invoicesRefreshAtom, (value) => value + 1)
  }
})

export const mergeInvoiceBatchEventAtom = atom(null, (get, set, batch: InvoiceBatchDetail) => {
  const active = get(activeInvoiceBatchAtom)
  if (active && active.id !== batch.id) return
  set(mergeInvoiceBatchAtom, batch)
})

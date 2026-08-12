import { useAtom, useAtomValue, useSetAtom } from "jotai"
import { lazy, Suspense, useCallback, useEffect, useState } from "react"
import { toast } from "sonner"

import { AppShell } from "@/components/app-shell"
import { InvoicesPage } from "@/components/invoices-page"
import { OrdersPage } from "@/components/orders-page"
import { Skeleton } from "@/components/ui/skeleton"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { api, errorMessage } from "@/lib/api"
import {
  activeBatchAtom,
  activeInvoiceBatchAtom,
  invoicesRefreshAtom,
  mergeBatchAtom,
  mergeBatchEventAtom,
  mergeInvoiceBatchAtom,
  mergeInvoiceBatchEventAtom,
  navigationAtom,
  ordersRefreshAtom,
  settingsAtom,
} from "@/lib/store"

const SettingsPage = lazy(() => import("@/components/settings-page"))
const ImportTasksPage = lazy(() => import("@/components/import-tasks-page"))
const MatchingPage = lazy(() => import("@/components/matching-page").then((module) => ({ default: module.MatchingPage })))

function Initializing() {
  return (
    <div className="flex min-h-screen bg-zinc-100">
      <div className="w-56 border-r bg-white p-4">
        <Skeleton className="h-10 w-36" />
        <Skeleton className="mt-8 h-9 w-full" />
        <Skeleton className="mt-2 h-9 w-full" />
      </div>
      <div className="flex-1 space-y-4 p-8">
        <Skeleton className="h-12 w-56" />
        <Skeleton className="h-[680px] w-full" />
      </div>
    </div>
  )
}

export function App() {
  const [view, setView] = useAtom(navigationAtom)
  const [settings, setSettings] = useAtom(settingsAtom)
  const [activeBatch, setActiveBatch] = useAtom(activeBatchAtom)
  const mergeBatch = useSetAtom(mergeBatchAtom)
  const mergeBatchEvent = useSetAtom(mergeBatchEventAtom)
  const refreshToken = useAtomValue(ordersRefreshAtom)
  const invoiceRefreshToken = useAtomValue(invoicesRefreshAtom)
  const [activeInvoiceBatch, setActiveInvoiceBatch] = useAtom(activeInvoiceBatchAtom)
  const mergeInvoiceBatch = useSetAtom(mergeInvoiceBatchAtom)
  const mergeInvoiceBatchEvent = useSetAtom(mergeInvoiceBatchEventAtom)
  const [ready, setReady] = useState(false)
  const [selectedBatchId, setSelectedBatchId] = useState<string | null>(null)
  const [selectedInvoiceBatchId, setSelectedInvoiceBatchId] = useState<string | null>(null)

  const handleBatchCreated = useCallback((batch: Parameters<typeof mergeBatch>[0]) => {
    mergeBatch(batch)
    setSelectedBatchId(batch.id)
    setView("importTasks")
  }, [mergeBatch, setView])

  const handleInvoiceBatchCreated = useCallback((batch: Parameters<typeof mergeInvoiceBatch>[0]) => {
    mergeInvoiceBatch(batch)
    setSelectedInvoiceBatchId(batch.id)
    setView("importTasks")
  }, [mergeInvoiceBatch, setView])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    let unlistenInvoice: (() => void) | undefined
    api
      .bootstrap()
      .then((state) => {
        if (disposed) return
        setSettings(state.settings)
        setActiveBatch(state.latestBatch)
        setActiveInvoiceBatch(state.latestInvoiceBatch)
        setReady(true)
      })
      .catch((error) => {
        toast.error(errorMessage(error), { duration: 8000 })
        setReady(true)
      })
    api.listenForTaskUpdates((event) => mergeBatchEvent(event.batch)).then((dispose) => {
      if (disposed) dispose()
      else unlisten = dispose
    })
    api.listenForInvoiceTaskUpdates((event) => mergeInvoiceBatchEvent(event.batch)).then((dispose) => {
      if (disposed) dispose()
      else unlistenInvoice = dispose
    })
    return () => {
      disposed = true
      unlisten?.()
      unlistenInvoice?.()
    }
  }, [mergeBatchEvent, mergeInvoiceBatchEvent, setActiveBatch, setActiveInvoiceBatch, setSettings])

  return (
    <TooltipProvider>
      {!ready ? (
        <Initializing />
      ) : (
        <AppShell view={view} onViewChange={setView}>
          {view === "orders" && (
            <OrdersPage
              onBatchCreated={handleBatchCreated}
              refreshToken={refreshToken}
            />
          )}
          {view === "invoices" && (
            <InvoicesPage
              onBatchCreated={handleInvoiceBatchCreated}
              refreshToken={invoiceRefreshToken}
            />
          )}
          {view === "matching" && (
            <Suspense fallback={<Initializing />}>
              <MatchingPage refreshToken={`${refreshToken}:${invoiceRefreshToken}`} />
            </Suspense>
          )}
          {view === "importTasks" && (
            <Suspense fallback={<Initializing />}>
              <ImportTasksPage
                activeBatch={activeBatch}
                selectedBatchId={selectedBatchId}
                onSelectedBatchChange={setSelectedBatchId}
                onBatchChange={mergeBatch}
                activeInvoiceBatch={activeInvoiceBatch}
                selectedInvoiceBatchId={selectedInvoiceBatchId}
                onSelectedInvoiceBatchChange={setSelectedInvoiceBatchId}
                onInvoiceBatchChange={(batch) => {
                  mergeInvoiceBatch(batch)
                  setActiveInvoiceBatch(batch)
                }}
              />
            </Suspense>
          )}
          {view === "settings" && settings && (
            <Suspense fallback={<Initializing />}>
              <SettingsPage settings={settings} onSettingsChange={setSettings} />
            </Suspense>
          )}
        </AppShell>
      )}
      <Toaster richColors position="bottom-right" />
    </TooltipProvider>
  )
}

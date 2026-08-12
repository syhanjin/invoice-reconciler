import { Provider } from "jotai"
import { createStore } from "jotai/vanilla"
import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { BatchDetail } from "@/lib/types"

const batch: BatchDetail = {
  id: "created-batch",
  platform: "taobao",
  status: "queued",
  createdAt: "2026-08-12T08:00:00Z",
  completedAt: null,
  counts: {
    total: 1,
    queued: 1,
    processing: 0,
    succeeded: 0,
    failed: 0,
    canceled: 0,
    skipped: 0,
    interrupted: 0,
  },
  tasks: [],
}

vi.mock("@/lib/api", () => ({
  api: {
    bootstrap: vi.fn().mockResolvedValue({
      settings: { ollamaBaseUrl: "http://127.0.0.1:11434", ollamaModel: "model", maxConcurrency: 3 },
      latestBatch: null,
    }),
    listenForTaskUpdates: vi.fn().mockResolvedValue(() => undefined),
  },
  errorMessage: () => "操作失败",
}))

vi.mock("@/components/app-shell", () => ({
  AppShell: ({ children }: { children: React.ReactNode }) => <main>{children}</main>,
}))

vi.mock("@/components/orders-page", () => ({
  OrdersPage: ({ onBatchCreated }: { onBatchCreated: (value: BatchDetail) => void }) => (
    <button type="button" onClick={() => onBatchCreated(batch)}>创建导入批次</button>
  ),
}))

vi.mock("@/components/import-tasks-page", () => ({
  default: ({ selectedBatchId }: { selectedBatchId: string | null }) => (
    <p>任务中心：{selectedBatchId}</p>
  ),
}))

import { App } from "./App"

describe("App import navigation", () => {
  afterEach(cleanup)

  it("opens the task center with the newly created batch selected", async () => {
    const user = userEvent.setup()
    render(<Provider store={createStore()}><App /></Provider>)

    await user.click(await screen.findByRole("button", { name: "创建导入批次" }))

    expect(await screen.findByText("任务中心：created-batch")).toBeInTheDocument()
  })
})

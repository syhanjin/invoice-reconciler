import { Provider } from "jotai"
import { createStore } from "jotai/vanilla"
import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
  api: {
    bootstrap: vi.fn().mockResolvedValue({
      settings: {
        llmProvider: "ollama",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "model",
        openaiBaseUrl: "https://example.com/v1",
        openaiModel: "vision-model",
        openaiApiKeyMasked: null,
        openaiApiKeyConfigured: false,
        orderRecognitionMaxConcurrency: 3,
        tencentSecretIdMasked: null,
        tencentCredentialsConfigured: false,
        invoiceOcrConcurrency: 3,
        matchMaxOrders: 10,
        matchMaxCandidates: 100,
        settingsPath: "/tmp/settings.toml",
        permissionWarning: null,
      },
      latestInvoiceBatch: null,
    }),
    listenForOrderImportUpdates: vi.fn().mockResolvedValue(() => undefined),
    listenForInvoiceTaskUpdates: vi.fn().mockResolvedValue(() => undefined),
    listenForMatchSummaryUpdates: vi.fn().mockResolvedValue(() => undefined),
  },
  errorMessage: () => "操作失败",
}))

vi.mock("@/components/app-shell", () => ({
  AppShell: ({ children }: { children: React.ReactNode }) => <main>{children}</main>,
}))

vi.mock("@/components/orders-page", () => ({
  OrdersPage: ({ onImagesImported }: { onImagesImported: () => void }) => (
    <button type="button" onClick={onImagesImported}>导入订单图片</button>
  ),
}))

vi.mock("@/components/import-tasks-page", () => ({
  default: () => <p>统一订单任务池</p>,
}))

import { App } from "./App"

describe("App import navigation", () => {
  afterEach(cleanup)

  it("opens the unified task pool after importing images", async () => {
    const user = userEvent.setup()
    render(<Provider store={createStore()}><App /></Provider>)

    await user.click(await screen.findByRole("button", { name: "导入订单图片" }))

    expect(await screen.findByText("统一订单任务池")).toBeInTheDocument()
  })
})

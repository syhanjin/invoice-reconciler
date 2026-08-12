import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { AppSettings } from "@/lib/types"

const mocks = vi.hoisted(() => ({
  saveSettings: vi.fn(),
  checkOllama: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
  api: mocks,
  errorMessage: () => "操作失败",
}))

import SettingsPage from "./settings-page"

const settings: AppSettings = {
  ollamaBaseUrl: "http://127.0.0.1:11434",
  ollamaModel: "qwen3-vl:4b",
  maxConcurrency: 3,
  tencentSecretIdMasked: "AKID****7890",
  tencentCredentialsConfigured: true,
  invoiceOcrConcurrency: 3,
  settingsPath: "/data/settings.toml",
  permissionWarning: null,
}

describe("SettingsPage", () => {
  afterEach(cleanup)
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.saveSettings.mockResolvedValue(settings)
  })

  it("keeps stored credentials when both secret inputs stay empty", async () => {
    const user = userEvent.setup()
    render(<SettingsPage settings={settings} onSettingsChange={vi.fn()} />)

    expect(screen.queryByDisplayValue("AKID****7890")).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "保存全部设置" }))

    await waitFor(() => expect(mocks.saveSettings).toHaveBeenCalledWith(expect.objectContaining({
      tencentSecretId: null,
      tencentSecretKey: null,
      clearTencentCredentials: false,
    })))
  })

  it("requires SecretId and SecretKey to be entered together", async () => {
    const user = userEvent.setup()
    render(<SettingsPage settings={settings} onSettingsChange={vi.fn()} />)

    await user.type(screen.getByLabelText("SecretId"), "AKID-new")
    await user.click(screen.getByRole("button", { name: "保存全部设置" }))

    expect(await screen.findByText("SecretId 和 SecretKey 必须同时填写")).toBeInTheDocument()
    expect(mocks.saveSettings).not.toHaveBeenCalled()
  })

  it("sends an explicit clear request without exposing the stored secret", async () => {
    const user = userEvent.setup()
    render(<SettingsPage settings={settings} onSettingsChange={vi.fn()} />)

    await user.click(screen.getByText("清除已保存的腾讯云凭据"))
    await user.click(screen.getByRole("button", { name: "保存全部设置" }))

    await waitFor(() => expect(mocks.saveSettings).toHaveBeenCalledWith(expect.objectContaining({
      tencentSecretId: null,
      tencentSecretKey: null,
      clearTencentCredentials: true,
    })))
  })

  it("keeps permission failures visible", () => {
    render(<SettingsPage settings={{ ...settings, permissionWarning: "无法将设置文件权限设为 0600" }} onSettingsChange={vi.fn()} />)
    expect(screen.getByText("设置文件权限未能收紧")).toBeInTheDocument()
    expect(screen.getByText("无法将设置文件权限设为 0600")).toBeInTheDocument()
  })
})

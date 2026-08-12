import { describe, expect, it } from "vitest"

import { parseYuanToFen } from "./format"

describe("parseYuanToFen", () => {
  it("converts decimal yuan without floating point multiplication", () => {
    expect(parseYuanToFen("19.99")).toBe(1999)
    expect(parseYuanToFen("1.5")).toBe(150)
  })

  it("supports blank optional amounts and rejects excess precision", () => {
    expect(parseYuanToFen("")).toBeNull()
    expect(parseYuanToFen("1.999")).toBeNaN()
  })
})

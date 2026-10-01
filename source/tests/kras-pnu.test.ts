import assert from "node:assert/strict"
import { describe, it } from "node:test"

import { isPnuInput, parsePnu, pnuToAddress } from "../scripts/kras-pnu.ts"

describe("isPnuInput", () => {
  it("accepts exactly 19 digits", () => {
    assert.equal(isPnuInput("1144012700104380000"), true)
  })

  it("rejects non-19-digit or non-numeric input", () => {
    assert.equal(isPnuInput("114401270010438000"), false)
    assert.equal(isPnuInput("11440127001043800000"), false)
    assert.equal(isPnuInput("1144012700O4380000"), false)
    assert.equal(isPnuInput("서울특별시 마포구 상암동 438"), false)
    assert.equal(isPnuInput(""), false)
  })
})

describe("parsePnu", () => {
  it("splits 법정동코드/대지구분/본번/부번", () => {
    assert.deepEqual(parsePnu("1141011800200110047"), {
      legalDongCode: "1141011800",
      landType: "mountain",
      mainNumber: 11,
      subNumber: 47,
    })
  })

  it("treats 대지구분 1 as plain and strips leading zeros", () => {
    assert.deepEqual(parsePnu("1144012700104380000"), {
      legalDongCode: "1144012700",
      landType: "plain",
      mainNumber: 438,
      subNumber: 0,
    })
  })

  it("throws on a non-19-digit PNU", () => {
    assert.throws(() => parsePnu("123"), /19자리/)
  })

  it("throws when 본번 is zero", () => {
    assert.throws(() => parsePnu("1144012700100000000"), /본번이 0/)
  })
})

describe("pnuToAddress", () => {
  it("converts a plain lot without 부번", () => {
    assert.equal(pnuToAddress("1144012700104380000"), "서울특별시 마포구 상암동 438")
  })

  it("converts a plain lot with 부번", () => {
    assert.equal(pnuToAddress("1117010200100470013"), "서울특별시 용산구 용산동2가 47-13")
  })

  it("converts a mountain lot (대지구분 2)", () => {
    assert.equal(pnuToAddress("1141011800200110047"), "서울특별시 서대문구 홍은동 산 11-47")
  })

  it("throws on an unknown 법정동코드", () => {
    assert.throws(() => pnuToAddress("9999999999100010000"), /알 수 없는 법정동코드/)
  })
})
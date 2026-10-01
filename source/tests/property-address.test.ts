import assert from "node:assert/strict"
import { describe, it } from "node:test"

import { PropertyAddressError, parsePropertyRow } from "../scripts/property-address.ts"

const NON_ADDRESS_CELL = "<not-address>"

function rowWithAddress(address: string): readonly string[] {
  return ["1", "공유재산", "토지", "강서구", "개화동", NON_ADDRESS_CELL, address, "미처리"]
}

describe("SCP property row address boundary", () => {
  it("returns normalized zero-based cell index 6 from an exact eight-string row", () => {
    // Given
    const row = rowWithAddress("  서울특별시\t강서구  개화동  255-1  ")

    // When
    const address = parsePropertyRow(row)

    // Then
    assert.equal(address, "서울특별시 강서구 개화동 255-1")
  })

  it("accepts ordinary Seoul mountain-lot forms", () => {
    // Given
    const rows = [
      rowWithAddress("서울특별시 종로구 청운동 산 1"),
      rowWithAddress("서울특별시 강서구 개화동 산255-1"),
    ]

    // When
    const addresses = rows.map(parsePropertyRow)

    // Then
    assert.deepEqual(addresses, [
      "서울특별시 종로구 청운동 산 1",
      "서울특별시 강서구 개화동 산 255-1",
    ])
  })

  it("accepts nationwide parcel addresses (Gyeonggi, metropolitan cities)", () => {
    // Given
    const rows = [
      rowWithAddress("경기도 군포시 금정동 849"),
      rowWithAddress("경기도 성남시 분당구 정자동 178-1"),
      rowWithAddress("부산광역시 해운대구 우동 1400"),
      rowWithAddress("경상북도 경주시 안강읍 옥산리 100-2"),
    ]

    // When
    const addresses = rows.map(parsePropertyRow)

    // Then
    assert.deepEqual(addresses, [
      "경기도 군포시 금정동 849",
      "경기도 성남시 분당구 정자동 178-1",
      "부산광역시 해운대구 우동 1400",
      "경상북도 경주시 안강읍 옥산리 100-2",
    ])
  })

  it("rejects rows that are not exact eight-string tuples", () => {
    // Given
    const invalidRows: readonly unknown[] = [
      rowWithAddress("서울특별시 강서구 개화동 255-1").slice(0, 7),
      [...rowWithAddress("서울특별시 강서구 개화동 255-1"), "extra"],
      ["1", "공유재산", "토지", "강서구", "개화동", NON_ADDRESS_CELL, 255, "미처리"],
      { address: "서울특별시 강서구 개화동 255-1" },
    ]

    // When / Then
    for (const row of invalidRows) {
      assert.throws(
        () => parsePropertyRow(row),
        (error: unknown) => error instanceof PropertyAddressError && error.code === "INVALID_ROW",
      )
    }
  })

  it("rejects non-parcel and malformed address values", () => {
    // Given
    const invalidAddresses = [
      "서울특별시 강서구 하늘길 38",
      "강서구 개화동 255-1",
      "서울특별시 강서구 개화동",
      "서울특별시 강서구 개화동 0",
      "서울특별시 강서구 개화동 255-0",
      "서울특별시 강서구 개화동 255-1 메모",
      `서울특별시 강서구 ${"가".repeat(150)}동 1`,
    ]

    // When / Then
    for (const address of invalidAddresses) {
      assert.throws(
        () => parsePropertyRow(rowWithAddress(address)),
        (error: unknown) =>
          error instanceof PropertyAddressError &&
          error.code === "INVALID_ADDRESS" &&
          !error.message.includes(address),
      )
    }
  })
})

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { lookupAfterNavigation } from "../scripts/kras-navigation.ts";
import { KrasLookupTargetError } from "../scripts/kras-page.ts";

const CERT_URL = "https://www.kras.go.kr/kras/cert/certView.do";

describe("lookupAfterNavigation", () => {
	it("waits for each document and retries the same lookup after context destruction", async () => {
		// Given
		const events: string[] = [];
		let calls = 0;
		const page = {
			url: () => CERT_URL,
			isClosed: () => false,
			waitForLoadState: async () => {
				events.push("ready");
			},
		};
		const lookup = async () => {
			events.push("lookup");
			if (++calls < 3)
				throw new Error(
					"page.evaluate: Execution context was destroyed, most likely because of a navigation",
				);
			return "same-parcel-result";
		};
		// When
		const result = await lookupAfterNavigation(page, lookup);
		// Then
		assert.equal(result, "same-parcel-result");
		assert.deepEqual(events, [
			"ready",
			"lookup",
			"ready",
			"lookup",
			"ready",
			"lookup",
		]);
	});

	for (const message of [
		"Execution context was destroyed",
		"report write failed",
	]) {
		it(`bounds or rejects ${message}`, async () => {
			// Given
			let calls = 0;
			const error = new Error(message);
			const page = {
				url: () => CERT_URL,
				isClosed: () => false,
				waitForLoadState: async () => undefined,
			};
			// When / Then
			await assert.rejects(
				lookupAfterNavigation(page, async () => {
					calls += 1;
					throw error;
				}),
				error,
			);
			assert.equal(
				calls,
				message === "Execution context was destroyed" ? 3 : 1,
			);
		});
	}

	it("does not replay a lookup when navigation ends at login", async () => {
		// Given
		let url = CERT_URL;
		let calls = 0;
		const page = {
			url: () => url,
			isClosed: () => false,
			waitForLoadState: async () => undefined,
		};
		// When / Then
		await assert.rejects(
			lookupAfterNavigation(page, async () => {
				calls += 1;
				url = "https://www.kras.go.kr/kras/login/intro.do";
				throw new Error("Execution context was destroyed");
			}),
			KrasLookupTargetError,
		);
		assert.equal(calls, 1);
	});
});

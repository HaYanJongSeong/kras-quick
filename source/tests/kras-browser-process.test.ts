import assert from "node:assert/strict";
import { describe, it } from "node:test";

import type { KrasBrowserChoice } from "../scripts/kras-browser-choice.ts";
import {
	ensureKrasChromium,
	KrasBrowserLaunchError,
	type KrasCdpOwner,
	matchesKrasBrowserOwner,
	readKrasCdpOwner,
	verifyKrasBrowserOwner,
} from "../scripts/kras-browser-process.ts";

const choice: KrasBrowserChoice = {
	engine: "chromium",
	label: "MOCK Chrome",
	executablePath: String.raw`C:\Program Files\Google\Chrome\Application\chrome.exe`,
	profileDirectory: String.raw`C:\KRAS Test\업무용 프로필`,
};
const owner = {
	executablePath: choice.executablePath,
	commandLine: `"${choice.executablePath}" --user-data-dir="${choice.profileDirectory}" --remote-debugging-port=9222`,
	addresses: ["127.0.0.1"],
} satisfies NonNullable<KrasCdpOwner>;

describe("matchesKrasBrowserOwner", () => {
	const accepted: readonly {
		readonly name: string;
		readonly candidate: NonNullable<KrasCdpOwner>;
	}[] = [
		{ name: "a quoted equals-form profile with spaces", candidate: owner },
		{
			name: "a separate quoted profile argument",
			candidate: {
				...owner,
				commandLine: `"${choice.executablePath}" --user-data-dir "${choice.profileDirectory}"`,
			},
		},
		{
			name: "a quoted whole equals-form argument",
			candidate: {
				...owner,
				commandLine: `"${choice.executablePath}" "--user-data-dir=${choice.profileDirectory}"`,
			},
		},
		{
			name: "case-insensitive Windows paths and normalized separators",
			candidate: {
				...owner,
				executablePath: "c:/PROGRAM FILES/Google/Chrome/Application/CHROME.EXE",
				commandLine:
					'chrome.exe --user-data-dir="c:/kras test/업무용 프로필/./"',
			},
		},
		{ name: "IPv6 loopback", candidate: { ...owner, addresses: ["::1"] } },
		{
			name: "both loopback families",
			candidate: { ...owner, addresses: ["127.0.0.1", "::1"] },
		},
	];
	for (const { name, candidate } of accepted) {
		it(`accepts ownership when given ${name}`, () => {
			// Given the selected executable and its isolated profile.
			const selected = choice;
			// When the listening process is checked.
			const matches = matchesKrasBrowserOwner(selected, candidate);
			// Then that exact owned listener may be reused.
			assert.equal(matches, true);
		});
	}

	const rejected: readonly {
		readonly name: string;
		readonly candidate: KrasCdpOwner;
	}[] = [
		{ name: "no listening process", candidate: null },
		{
			name: "another executable",
			candidate: { ...owner, executablePath: String.raw`C:\Other\chrome.exe` },
		},
		{
			name: "an executable-name suffix",
			candidate: {
				...owner,
				executablePath: `${choice.executablePath}.other.exe`,
			},
		},
		{
			name: "another profile of the selected executable",
			candidate: {
				...owner,
				commandLine: 'chrome.exe --user-data-dir="C:\\Users\\MOCK\\personal"',
			},
		},
		{
			name: "a profile sharing only the expected prefix",
			candidate: {
				...owner,
				commandLine: `chrome.exe --user-data-dir="${choice.profileDirectory}-other"`,
			},
		},
		{
			name: "a missing profile flag",
			candidate: {
				...owner,
				commandLine: 'chrome.exe --profile-directory="Default"',
			},
		},
		{
			name: "a missing separate profile value",
			candidate: { ...owner, commandLine: "chrome.exe --user-data-dir" },
		},
		{
			name: "a similarly named flag",
			candidate: {
				...owner,
				commandLine: `chrome.exe --other-user-data-dir="${choice.profileDirectory}"`,
			},
		},
		...["0.0.0.0", "::", "192.0.2.8", "::ffff:127.0.0.1"].map((address) => ({
			name: `a non-approved listener address ${address}`,
			candidate: { ...owner, addresses: [address] },
		})),
		{
			name: "a public binding alongside a loopback binding",
			candidate: { ...owner, addresses: ["127.0.0.1", "0.0.0.0"] },
		},
	];
	for (const { name, candidate } of rejected) {
		it(`rejects ownership when given ${name}`, () => {
			// Given a process that must not be controlled.
			const selected = choice;
			// When it is compared with the selection.
			const matches = matchesKrasBrowserOwner(selected, candidate);
			// Then it cannot be reused or implicitly treated as owned.
			assert.equal(matches, false);
		});
	}
});

describe("KRAS local endpoint guard", () => {
	for (const endpoint of [
		"https://127.0.0.1:9222",
		"http://192.0.2.8:9222",
		"http://example.invalid:9222",
		"http://127.0.0.1.example.invalid:9222",
		"http://0.0.0.0:9222",
		"http://[::]:9222",
	]) {
		for (const operation of [verifyKrasBrowserOwner, ensureKrasChromium]) {
			it(`${operation.name} rejects ${endpoint} before querying or launching a process`, async () => {
				// Given an endpoint outside the local HTTP boundary.
				const selected = choice;
				// When either connection path is requested.
				const result = operation(selected, endpoint);
				// Then the domain guard, not PowerShell or a browser, rejects it.
				await assert.rejects(result, KrasBrowserLaunchError);
			});
		}
	}

	for (const port of [
		0,
		-1,
		65536,
		1.5,
		Number.NaN,
		Number.POSITIVE_INFINITY,
	]) {
		it(`rejects invalid port ${port} before invoking PowerShell`, async () => {
			// Given an invalid TCP port.
			const invalidPort = port;
			// When ownership discovery is requested.
			const result = readKrasCdpOwner(invalidPort);
			// Then no platform-specific process query is needed.
			await assert.rejects(result, KrasBrowserLaunchError);
		});
	}
});

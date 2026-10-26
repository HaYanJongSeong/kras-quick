import {
	type BrowserContext,
	chromium,
	firefox,
	type Page,
} from "playwright-core";

import {
	chooseKrasBrowser,
	discoverKrasBrowsers,
	type KrasBrowserChoice,
} from "./kras-browser-choice.ts";
import {
	ensureKrasChromium,
	KrasBrowserLaunchError,
	verifyKrasBrowserOwner,
} from "./kras-browser-process.ts";
import type { TerminalReader } from "./terminal-reader.ts";

export type KrasBrowserLease = {
	readonly context: BrowserContext;
	readonly pages: readonly Page[];
	readonly release: () => Promise<void>;
};

let selectedBrowser: KrasBrowserChoice | undefined;
let firefoxContext: Promise<BrowserContext> | undefined;

function cdpEndpoint(): string {
	return process.env["KRAS_CDP_ENDPOINT"] ?? "http://127.0.0.1:9222";
}

export async function prepareKrasQuickBrowser(
	terminal: TerminalReader,
	rootDirectory: string,
): Promise<boolean> {
	const choice = await chooseKrasBrowser(
		terminal,
		discoverKrasBrowsers(rootDirectory),
		rootDirectory,
	);
	if (choice === undefined) return false;
	selectedBrowser = choice;
	switch (choice.engine) {
		case "chromium":
			await ensureKrasChromium(choice, cdpEndpoint());
			break;
		case "firefox":
			process.stdout.write(
				"Firefox는 시험 지원입니다. 프로그램 종료 시 업무용 Firefox 창도 닫힙니다.\n",
			);
			break;
		default:
			throw new KrasBrowserLaunchError(
				`알 수 없는 브라우저 엔진: ${choice.engine satisfies never}`,
			);
	}
	process.stdout.write(`선택한 브라우저: ${choice.label}\n`);
	return true;
}

export async function acquireKrasBrowser(): Promise<KrasBrowserLease> {
	if (selectedBrowser?.engine === "firefox") {
		if (firefoxContext === undefined) {
			// moz-firefox is Playwright 1.61.1's undocumented BiDi path for stock Firefox.
			firefoxContext = firefox
				.launchPersistentContext(selectedBrowser.profileDirectory, {
					channel: "moz-firefox",
					executablePath: selectedBrowser.executablePath,
					headless: false,
					locale: "ko-KR",
					viewport: null,
					timeout: 30_000,
				})
				.catch((error: unknown) => {
					firefoxContext = undefined;
					throw error;
				});
		}
		const context = await firefoxContext;
		return { context, pages: context.pages(), release: async () => undefined };
	}
	if (selectedBrowser !== undefined)
		await verifyKrasBrowserOwner(selectedBrowser, cdpEndpoint());
	const browser = await chromium.connectOverCDP(cdpEndpoint(), {
		noDefaults: true,
		timeout: 5000,
	});
	try {
		const context = browser.contexts()[0] ?? (await browser.newContext());
		return {
			context,
			pages: browser.contexts().flatMap((candidate) => candidate.pages()),
			release: () => browser.close(),
		};
	} catch (error: unknown) {
		await browser.close();
		throw error;
	}
}

export async function shutdownKrasBrowser(): Promise<void> {
	const context = firefoxContext;
	firefoxContext = undefined;
	selectedBrowser = undefined;
	if (context !== undefined) await (await context).close();
}

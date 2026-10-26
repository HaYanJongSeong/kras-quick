import assert from "node:assert/strict";
import { tmpdir } from "node:os";
import { describe, it } from "node:test";
import { PDFDocument } from "pdf-lib";
import { chromium } from "playwright-core";
import {
	acquireKrasBrowser,
	shutdownKrasBrowser,
} from "../scripts/kras-browser.ts";
import { discoverKrasBrowsers } from "../scripts/kras-browser-choice.ts";
import { connectKrasOzViewer } from "../scripts/kras-oz.ts";
import {
	connectKrasWorkflowWatcher,
	connectOrOpenKrasWorkflowWatcher,
} from "../scripts/kras-workflow-watcher.ts";
import {
	ANYID_URL,
	CERT_URL,
	installKrasFixtures,
	MOCK_XML,
	OZ_URL,
	POPUP_URL,
	withKrasBrowser,
} from "./kras-browser-fixture.ts";

const enabled =
	process.platform === "win32" &&
	process.env["KRAS_BROWSER_INTEGRATION"] === "1";
const installed = enabled ? discoverKrasBrowsers(tmpdir()) : [];

describe("KRAS real Windows browser connectors with MOCK-only pages", {
	skip: !enabled,
	concurrency: false,
}, () => {
	it("finds at least one installed Chromium-family browser or Firefox when explicitly enabled", () => {
		// Given native discovery, not downloaded or substituted browser binaries.
		const choices = installed;
		// When the opt-in integration suite inspects the available targets.
		const count = choices.length;
		// Then a green run cannot silently mean that no real browser was exercised.
		assert.ok(
			count > 0,
			"Install a supported Chromium-family browser or Firefox before opting into integration",
		);
	});

	for (const installedChoice of installed) {
		describe(`${installedChoice.label}: ${installedChoice.executablePath}`, {
			concurrency: false,
		}, () => {
			it("reuses concurrent Firefox acquisitions and shuts down only its owned Firefox context", async () => {
				// Given a menu-selected browser with an isolated profile and, for Chromium, an owned control context.
				await withKrasBrowser(installedChoice, async ({ choice, control }) => {
					// When two clients acquire the selected browser concurrently.
					const [first, second] = await Promise.all([
						acquireKrasBrowser(),
						acquireKrasBrowser(),
					]);
					// Then Firefox shares the one persistent context rather than racing two launches.
					switch (choice.engine) {
						case "firefox":
							assert.equal(first.context, second.context);
							break;
						case "chromium":
							assert.ok(control !== undefined);
							break;
						default:
							assert.fail(
								`Unsupported test engine: ${choice.engine satisfies never}`,
							);
					}
					const observer = control ?? first.context;
					const page = observer.pages()[0] ?? (await observer.newPage());
					await first.release();
					await second.release();
					assert.equal(await page.evaluate(() => 40 + 2), 42);
					let contextClosed = false;
					observer.once("close", () => {
						contextClosed = true;
					});
					await shutdownKrasBrowser();
					switch (choice.engine) {
						case "firefox":
							assert.equal(contextClosed, true);
							assert.equal(page.isClosed(), true);
							break;
						case "chromium":
							assert.equal(contextClosed, false);
							assert.equal(await page.evaluate(() => 40 + 2), 42);
							break;
						default:
							assert.fail(
								`Unsupported test engine: ${choice.engine satisfies never}`,
							);
					}
				});
			});

			it("uses workflow adapters and preserves opener DOM and cross-origin messages across disconnects", async () => {
				// Given intercepted MOCK transport pages and a retained lease to install context-wide routes.
				await withKrasBrowser(installedChoice, async ({ control }) => {
					const lease = await acquireKrasBrowser();
					const unexpected = await installKrasFixtures(lease.context);
					const observer = control ?? lease.context;
					// When the real adapter opens its missing target, then another adapter attaches to it.
					const opened = await connectOrOpenKrasWorkflowWatcher();
					const attached = await connectKrasWorkflowWatcher();
					assert.equal(opened.url(), CERT_URL);
					assert.equal(attached.url(), CERT_URL);
					const page = lease.context
						.pages()
						.find((candidate) => candidate.url() === CERT_URL);
					assert.ok(page !== undefined);
					await page.locator("#mock-field").fill("MOCK_DOM_SENTINEL");
					assert.deepEqual(await attached.readBuildingOptions(), [
						{ index: 1, value: "MOCK_A", label: "MOCK A" },
						{ index: 2, value: "MOCK_B", label: "MOCK B" },
					]);
					assert.equal(
						await attached.fillCaptchaAndSubmit("MOCK_NOT_A_CREDENTIAL"),
						"success",
					);
					assert.equal(
						await page.locator("#captcha").inputValue(),
						"MOCK_NOT_A_CREDENTIAL",
					);
					const [popup] = await Promise.all([
						page.waitForEvent("popup"),
						page.locator("#mock-popup").click(),
					]);
					await popup.waitForLoadState("domcontentloaded");
					assert.equal(await popup.opener(), page);
					assert.equal(
						await popup.locator("#mock-opener").textContent(),
						"MOCK_DOM_SENTINEL",
					);
					assert.equal(
						await page.locator("body").getAttribute("data-mock-popup"),
						"MOCK_DOM_SEEN",
					);
					const [messagePopup] = await Promise.all([
						page.waitForEvent("popup"),
						page.locator("#mock-anyid").click(),
					]);
					await messagePopup.waitForLoadState("domcontentloaded");
					await page.waitForFunction(
						() =>
							document.querySelector("#mock-message")?.textContent ===
							"MOCK_SOP_BLOCKED",
					);
					assert.equal(await messagePopup.opener(), page);
					assert.ok(
						opened.disconnect !== undefined &&
							attached.disconnect !== undefined,
					);
					await opened.disconnect();
					await attached.disconnect();
					await lease.release();
					// Then release preserves the parent and both popup relationships, including on stock Firefox.
					for (const url of [CERT_URL, POPUP_URL, ANYID_URL]) {
						const surviving = observer
							.pages()
							.find((candidate) => candidate.url() === url);
						assert.ok(surviving !== undefined);
						assert.equal(surviving.isClosed(), false);
						assert.ok(
							await surviving.locator("body").getAttribute("data-mock"),
						);
					}
					const parent = observer
						.pages()
						.find((candidate) => candidate.url() === CERT_URL);
					assert.ok(parent !== undefined);
					assert.equal(
						await parent.locator("#mock-field").inputValue(),
						"MOCK_DOM_SENTINEL",
					);
					assert.deepEqual(unexpected, []);
				});
			});

			it("captures and closes the current two-page report while preserving an older report tab", async (t) => {
				// Given a synthetic two-page canvas popup, never a real report or a page.pdf implementation.
				await withKrasBrowser(installedChoice, async ({ control }) => {
					const lease = await acquireKrasBrowser();
					const unexpected = await installKrasFixtures(lease.context);
					const observer = control ?? lease.context;
					const parent =
						lease.context.pages()[0] ?? (await lease.context.newPage());
					await parent.goto(CERT_URL);
					const stale = await lease.context.newPage();
					await stale.goto(OZ_URL);
					await stale.evaluate(() => {
						Reflect.set(window, "ozData", "<STALE_REPORT/>");
					});
					const watcher = await connectKrasWorkflowWatcher();
					const [popup] = await Promise.all([
						parent.waitForEvent("popup"),
						parent.locator("#mock-oz").click(),
					]);
					await popup.waitForLoadState("domcontentloaded");
					const connect = chromium.connectOverCDP.bind(chromium);
					t.mock.method(
						chromium,
						"connectOverCDP",
						async (...args: Parameters<typeof connect>) => {
							const browser = await connect(...args);
							for (const context of browser.contexts())
								context.setDefaultTimeout(100);
							return browser;
						},
					);
					lease.context.setDefaultTimeout(100);
					// When the actual OZ adapter captures its PNG-backed artifacts.
					assert.equal(await watcher.awaitOzViewer(), true);
					const viewer = await (
						watcher.connectOzViewer ?? connectKrasOzViewer
					)();
					assert.ok(viewer !== undefined);
					assert.equal(viewer.url(), OZ_URL);
					assert.equal(await viewer.hasVisibleCaptcha(), false);
					await popup.evaluate(() => {
						const canvas = document.querySelector<HTMLCanvasElement>("canvas");
						if (canvas === null) throw new Error("MOCK canvas missing");
						canvas.width = 0;
						setTimeout(() => {
							canvas.width = 64;
							document
								.querySelector("select")
								?.dispatchEvent(new Event("change"));
						}, 400);
					});
					const pngs = await viewer.capturePagePngs();
					const pdf = await PDFDocument.load(await viewer.capturePdf());
					const xml = await viewer.readReportXml();
					// Then both pages contain real PNG bytes and produce an independently readable PDF.
					assert.equal(pngs.length, 2);
					const [first, second] = pngs;
					assert.ok(
						first instanceof Uint8Array && second instanceof Uint8Array,
					);
					for (const png of [first, second]) {
						assert.deepEqual(
							[...png.subarray(0, 8)],
							[137, 80, 78, 71, 13, 10, 26, 10],
						);
						assert.ok(png.byteLength > 8);
					}
					assert.notDeepEqual(Buffer.from(first), Buffer.from(second));
					assert.equal(pdf.getPageCount(), 2);
					assert.deepEqual(
						pdf.getPages().map((page) => page.getSize()),
						[
							{ width: 64, height: 48 },
							{ width: 64, height: 48 },
						],
					);
					assert.equal(xml, MOCK_XML);
					assert.deepEqual(await viewer.capturePagePngs(), pngs);
					assert.ok(
						viewer.disconnect !== undefined && watcher.disconnect !== undefined,
					);
					await viewer.disconnect();
					const closing = popup.waitForEvent("close");
					assert.equal(await watcher.closeOzViewer(), true);
					await closing;
					assert.equal(stale.isClosed(), false);
					assert.equal(
						await stale.evaluate(() => Reflect.get(window, "ozData")),
						"<STALE_REPORT/>",
					);
					await watcher.disconnect();
					await lease.release();
					const surviving = observer
						.pages()
						.find((candidate) => candidate.url() === OZ_URL);
					assert.ok(surviving !== undefined);
					assert.equal(await surviving.locator("canvas").count(), 1);
					assert.deepEqual(unexpected, []);
				});
			});

			it("rejects ambiguous report selection without closing either existing report", async () => {
				// Given two previously opened reports in an isolated MOCK browser.
				await withKrasBrowser(installedChoice, async () => {
					const lease = await acquireKrasBrowser();
					const unexpected = await installKrasFixtures(lease.context);
					const parent =
						lease.context.pages()[0] ?? (await lease.context.newPage());
					await parent.goto(CERT_URL);
					const first = await lease.context.newPage();
					await first.goto(OZ_URL);
					const second = await lease.context.newPage();
					await second.goto(OZ_URL);
					// When no current workflow identifies either report.
					await assert.rejects(connectKrasOzViewer(), {
						name: "KrasOzViewerSelectionError",
					});
					// Then failure is explicit and no report is destroyed or guessed.
					assert.equal(first.isClosed(), false);
					assert.equal(second.isClosed(), false);
					assert.deepEqual(unexpected, []);
					await lease.release();
				});
			});

			it("does not reuse or close an old report after a new submission without a new viewer", async () => {
				// Given an old report and a new MOCK submission that opens no report.
				await withKrasBrowser(installedChoice, async () => {
					const lease = await acquireKrasBrowser();
					const unexpected = await installKrasFixtures(lease.context);
					const parent =
						lease.context.pages()[0] ?? (await lease.context.newPage());
					await parent.goto(CERT_URL);
					const stale = await lease.context.newPage();
					await stale.goto(OZ_URL);
					const watcher = await connectKrasWorkflowWatcher();
					assert.equal(
						await watcher.fillCaptchaAndSubmit("MOCK_NOT_A_CREDENTIAL"),
						"success",
					);
					// When capture and close are requested without a fresh report.
					assert.ok(watcher.connectOzViewer !== undefined);
					const viewer = await watcher.connectOzViewer();
					const closed = await watcher.closeOzViewer();
					// Then neither operation substitutes the old report.
					assert.equal(viewer, undefined);
					assert.equal(closed, false);
					assert.equal(stale.isClosed(), false);
					assert.deepEqual(unexpected, []);
					await watcher.disconnect?.();
					await lease.release();
				});
			});

			it("captures a named report tab when a new submission reloads the same Page", async () => {
				// Given a previous report tab, retained for a named-window reuse.
				await withKrasBrowser(installedChoice, async () => {
					const lease = await acquireKrasBrowser();
					const unexpected = await installKrasFixtures(lease.context);
					const parent =
						lease.context.pages()[0] ?? (await lease.context.newPage());
					await parent.goto(CERT_URL);
					const reused = await lease.context.newPage();
					await reused.goto(OZ_URL);
					const watcher = await connectKrasWorkflowWatcher();
					assert.equal(
						await watcher.fillCaptchaAndSubmit("MOCK_NOT_A_CREDENTIAL"),
						"success",
					);
					// When the report is refreshed in the same Page instead of a new popup.
					await reused.reload();
					assert.equal(await watcher.awaitOzViewer(), true);
					assert.ok(watcher.connectOzViewer !== undefined);
					const viewer = await watcher.connectOzViewer();
					// Then the refreshed report is usable and its lease remains workflow-owned.
					assert.ok(viewer !== undefined);
					assert.equal(await viewer.readReportXml(), MOCK_XML);
					await viewer.disconnect?.();
					assert.equal(reused.isClosed(), false);
					assert.deepEqual(unexpected, []);
					await watcher.disconnect?.();
					await lease.release();
				});
			});

			it("recaptures the sole existing report when manual mode explicitly allows it", async () => {
				// Given one report that was opened before this workflow adapter.
				await withKrasBrowser(installedChoice, async () => {
					const lease = await acquireKrasBrowser();
					const unexpected = await installKrasFixtures(lease.context);
					const parent =
						lease.context.pages()[0] ?? (await lease.context.newPage());
					await parent.goto(CERT_URL);
					const existing = await lease.context.newPage();
					await existing.goto(OZ_URL);
					const watcher = await connectKrasWorkflowWatcher();
					// When Quick 0 explicitly requests the already-open report.
					assert.equal(
						await watcher.awaitOzViewer({ allowExisting: true }),
						true,
					);
					assert.ok(watcher.connectOzViewer !== undefined);
					const viewer = await watcher.connectOzViewer();
					// Then its actual pixels and XML remain capturable without reopening it.
					assert.ok(viewer !== undefined);
					assert.equal((await viewer.capturePagePngs()).length, 2);
					assert.equal(await viewer.readReportXml(), MOCK_XML);
					await viewer.disconnect?.();
					assert.equal(existing.isClosed(), false);
					assert.deepEqual(unexpected, []);
					await watcher.disconnect?.();
					await lease.release();
				});
			});

			it("releases missing-target adapter connections without closing the retained browser page", async () => {
				// Given a selected browser containing only an unrelated blank parent.
				await withKrasBrowser(installedChoice, async ({ control }) => {
					const lease = await acquireKrasBrowser();
					const parent = (control ?? lease.context).pages()[0];
					assert.ok(parent !== undefined);
					// When both adapters fail to find their exact target.
					await assert.rejects(connectKrasWorkflowWatcher(), Error);
					const viewer = await connectKrasOzViewer();
					// Then the error/empty result is observable and neither release destroys the parent.
					assert.equal(viewer, undefined);
					assert.equal(parent.isClosed(), false);
					assert.equal(await parent.evaluate(() => 6 * 7), 42);
					await lease.release();
					assert.equal(await parent.evaluate(() => 6 * 7), 42);
				});
			});
		});
	}
});

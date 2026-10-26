import assert from "node:assert/strict";
import { tmpdir } from "node:os";
import { describe, it } from "node:test";
import { PDFDocument } from "pdf-lib";
import { acquireKrasBrowser } from "../scripts/kras-browser.ts";
import { discoverKrasBrowsers } from "../scripts/kras-browser-choice.ts";
import { createKrasOzViewerSurface } from "../scripts/kras-oz.ts";
import {
	installKrasFixtures,
	OZ_URL,
	withKrasBrowser,
} from "./kras-browser-fixture.ts";

const enabled =
	process.platform === "win32" &&
	process.env["KRAS_BROWSER_INTEGRATION"] === "1";
const installed = enabled
	? discoverKrasBrowsers(tmpdir()).filter(
			(choice) => choice.engine === "firefox",
		)
	: [];

describe("KRAS real Firefox OZ locale captures with MOCK-only pages", {
	skip: !enabled,
	concurrency: false,
}, () => {
	it("finds installed Firefox when explicitly enabled", () => {
		// Given native Firefox discovery, not a downloaded replacement.
		const choices = installed;
		// When the opt-in suite counts its available targets.
		const count = choices.length;
		// Then a green run cannot silently exercise zero Firefox installations.
		assert.ok(
			count > 0,
			"Install Firefox before opting into locale integration",
		);
	});

	for (const installedChoice of installed) {
		for (const language of ["ko", "en"] as const) {
			it(`${installedChoice.label}: captures both ${language} pages from page 2 and caches PDF pixels`, async () => {
				// Given intercepted synthetic pixels in an isolated real Firefox profile.
				await withKrasBrowser(installedChoice, async () => {
					const lease = await acquireKrasBrowser();
					try {
						const unexpected = await installKrasFixtures(
							lease.context,
							language,
						);
						const page =
							lease.context.pages()[0] ?? (await lease.context.newPage());
						await page.goto(OZ_URL);
						assert.equal(
							await page.evaluate(() => navigator.language),
							"ko-KR",
						);
						const [currentTitle, previousTitle, nextTitle] = await page
							.locator("input[title]")
							.evaluateAll((inputs) =>
								inputs.map((input) => input.getAttribute("title")),
							);
						assert.ok(
							typeof currentTitle === "string" &&
								typeof previousTitle === "string" &&
								typeof nextTitle === "string",
						);
						for (const title of [currentTitle, previousTitle, nextTitle]) {
							assert.ok(title.length > 0);
							assert.equal(/[가-힣]/u.test(title), language === "ko");
						}
						const current = page.getByTitle(currentTitle, { exact: true });
						await page.getByTitle(nextTitle, { exact: true }).click();
						assert.equal(await current.inputValue(), "2");
						const viewer = createKrasOzViewerSurface(page);

						// When capture resets via previous, advances via next, and PDF reuses cached PNGs.
						const pngs = await viewer.capturePagePngs();
						await page.locator("canvas").evaluate((canvas) => {
							canvas.setAttribute("width", "1");
							canvas.setAttribute("height", "1");
						});
						const pdf = await PDFDocument.load(await viewer.capturePdf());

						// Then distinct full-size artifacts survive without closing the report or using the network.
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
							pdf.getPages().map((pdfPage) => pdfPage.getSize()),
							[
								{ width: 64, height: 48 },
								{ width: 64, height: 48 },
							],
						);
						assert.equal(await current.inputValue(), "2");
						assert.equal(page.isClosed(), false);
						assert.equal(page.url(), OZ_URL);
						assert.deepEqual(unexpected, []);
					} finally {
						await lease.release();
					}
				});
			});
		}
	}
});

import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type BrowserContext, chromium } from "playwright-core";
import {
	prepareKrasQuickBrowser,
	shutdownKrasBrowser,
} from "../scripts/kras-browser.ts";
import {
	discoverKrasBrowsers,
	type KrasBrowserChoice,
} from "../scripts/kras-browser-choice.ts";
import {
	matchesKrasBrowserOwner,
	readKrasCdpOwner,
} from "../scripts/kras-browser-process.ts";
import type { TerminalReader } from "../scripts/terminal-reader.ts";

export const CERT_URL = "https://www.kras.go.kr/kras/cert/certView.do";
export const POPUP_URL = "https://www.kras.go.kr/kras/cert/mock-popup";
export const ANYID_URL = "https://anyid.mock.invalid/transport";
export const OZ_URL = "https://kras.go.kr/oz80/ozhviewer/report.jsp";
export const MOCK_XML = "<MOCK_REPORT><PAGE_COUNT>2</PAGE_COUNT></MOCK_REPORT>";

type PreparedBrowser = {
	readonly choice: KrasBrowserChoice;
	readonly control: BrowserContext | undefined;
};

async function availablePort(): Promise<number> {
	const server = createServer();
	const listening = once(server, "listening");
	server.listen(0, "127.0.0.1");
	await listening;
	try {
		const address = server.address();
		assert.ok(address !== null && typeof address !== "string");
		return address.port;
	} finally {
		await new Promise<void>((resolve, reject) => {
			server.close((error) =>
				error === undefined ? resolve() : reject(error),
			);
		});
	}
}

export async function withKrasBrowser(
	installed: KrasBrowserChoice,
	run: (browser: PreparedBrowser) => Promise<void>,
): Promise<void> {
	const root = await mkdtemp(join(tmpdir(), "kras-browser-integration-"));
	const previousEndpoint = process.env["KRAS_CDP_ENDPOINT"];
	let control: BrowserContext | undefined;
	try {
		await shutdownKrasBrowser();
		const candidates = discoverKrasBrowsers(root);
		const index = candidates.findIndex(
			(candidate) => candidate.executablePath === installed.executablePath,
		);
		const choice = candidates[index];
		assert.ok(
			choice !== undefined,
			"The installed browser must still be discoverable",
		);
		switch (choice.engine) {
			case "chromium": {
				const port = await availablePort();
				process.env["KRAS_CDP_ENDPOINT"] = `http://127.0.0.1:${port}`;
				control = await chromium.launchPersistentContext(
					choice.profileDirectory,
					{
						executablePath: choice.executablePath,
						headless: false,
						viewport: null,
						timeout: 30_000,
						args: [
							`--remote-debugging-port=${port}`,
							"--remote-debugging-address=127.0.0.1",
						],
					},
				);
				// Never let prepare spawn its real KRAS URL if pipe + explicit CDP port is unsupported.
				assert.ok(
					matchesKrasBrowserOwner(choice, await readKrasCdpOwner(port)),
					"Playwright pipe + explicit CDP port must expose the exact owned executable/profile before prepare",
				);
				break;
			}
			case "firefox":
				// A stray Chromium fallback must not reach any default debugging endpoint.
				process.env["KRAS_CDP_ENDPOINT"] =
					"http://browser-fallback.mock.invalid:1";
				break;
			default:
				assert.fail(
					`Unsupported fixture engine: ${choice.engine satisfies never}`,
				);
		}
		let questions = 0;
		const terminal: TerminalReader = {
			question: async () => {
				questions += 1;
				assert.equal(
					questions,
					1,
					"prepare must use the discovered choice, not manual entry",
				);
				return String(index + 1);
			},
			close: () => undefined,
		};
		assert.equal(await prepareKrasQuickBrowser(terminal, root), true);
		assert.equal(questions, 1);
		await run({ choice, control });
	} finally {
		try {
			await shutdownKrasBrowser();
		} finally {
			try {
				if (control !== undefined) await control.close();
			} finally {
				if (previousEndpoint === undefined)
					delete process.env["KRAS_CDP_ENDPOINT"];
				else process.env["KRAS_CDP_ENDPOINT"] = previousEndpoint;
				await rm(root, { recursive: true, force: true, maxRetries: 5 });
			}
		}
	}
}

const certificate = `<!doctype html><html><head><title>MOCK certificate transport</title>
<link rel="icon" href="data:,"></head><body data-mock="certificate">
<h1>MOCK ONLY: no authentication, credentials, property lookup or real results</h1>
<label>MOCK input <input id="mock-field"></label>
<label>MOCK adapter input, not a challenge <input id="captcha"></label>
<select id="bldgSeCd"><option value="">선택</option>
<option value="MOCK_A">MOCK A</option><option value="MOCK_B">MOCK B</option></select>
<button onclick="certView.fnProcessCert();">MOCK submit</button>
<button id="mock-popup" onclick="window.open('${POPUP_URL}', 'MOCK_OPENER')">MOCK opener</button>
<button id="mock-anyid" onclick="window.open('${ANYID_URL}', 'MOCK_ANYID')">MOCK message transport</button>
<button id="mock-oz" onclick="window.open('${OZ_URL}', 'MOCK_OZ')">MOCK canvas report</button>
<output id="mock-message"></output><script>
var certView = { fnProcessCert: function () {
  const dialog = document.createElement('div');
  dialog.className = 'swal2-container swal2-backdrop-show';
  dialog.textContent = 'MOCK 도면정보';
  const button = document.createElement('button');
  button.className = 'swal2-confirm'; button.textContent = 'MOCK acknowledge';
  button.onclick = () => dialog.remove(); dialog.append(button); document.body.append(dialog);
} };
addEventListener('message', event => {
  if (event.origin === 'https://anyid.mock.invalid' && event.data === 'MOCK_SOP_BLOCKED') {
    document.getElementById('mock-message').textContent = event.data;
  }
});
</script></body></html>`;

const opener = `<!doctype html><html><head><title>MOCK same-origin opener</title>
<link rel="icon" href="data:,"></head><body data-mock="opener"><p>MOCK ONLY</p>
<output id="mock-opener"></output><script>
document.getElementById('mock-opener').textContent = window.opener.document.getElementById('mock-field').value;
window.opener.document.body.dataset.mockPopup = 'MOCK_DOM_SEEN';
</script></body></html>`;

const anyid = `<!doctype html><html><head><title>MOCK cross-origin transport, not AnyID authentication</title>
<link rel="icon" href="data:,"></head><body data-mock="anyid"><p>MOCK ONLY: no login</p><script>
let result = 'MOCK_SOP_BROKEN';
try { window.opener.document; } catch (error) {
  if (error instanceof DOMException && error.name === 'SecurityError') result = 'MOCK_SOP_BLOCKED';
}
window.opener.postMessage(result, 'https://www.kras.go.kr');
</script></body></html>`;

const oz = `<!doctype html><html><head><title>MOCK two-page canvas, not a certificate</title>
<link rel="icon" href="data:,"></head><body data-mock="oz"><p>MOCK ONLY: synthetic pixels and XML</p>
<select class="oz_ui_layout"><option>100%</option><option>300%</option></select>
<span id="mock-pages">1 / 2</span><input title="현재 페이지" value="1" readonly>
<input type="button" title="한 페이지 전으로 이동" value="MOCK previous">
<input type="button" title="한 페이지 다음으로 이동" value="MOCK next">
<canvas width="64" height="48"></canvas><script>
var ozData = '${MOCK_XML}';
let current = 1;
function draw() {
  const canvas = document.querySelector('canvas'); const context = canvas.getContext('2d');
  context.fillStyle = current === 1 ? '#c02020' : '#2040c0'; context.fillRect(0, 0, 64, 48);
  context.fillStyle = '#ffffff'; context.fillRect(current * 8, 8, 8, 8);
  document.querySelector('input[title="현재 페이지"]').value = String(current);
  document.getElementById('mock-pages').textContent = current + ' / 2';
}
document.querySelector('select').onchange = draw;
document.querySelector('input[title="한 페이지 전으로 이동"]').onclick = () => { current = Math.max(1, current - 1); draw(); };
document.querySelector('input[title="한 페이지 다음으로 이동"]').onclick = () => { current = Math.min(2, current + 1); draw(); };
draw();
</script></body></html>`;

export async function installKrasFixtures(
	context: BrowserContext,
	language: "ko" | "en" = "ko",
): Promise<readonly string[]> {
	const responses = new Map([
		[CERT_URL, certificate],
		[POPUP_URL, opener],
		[ANYID_URL, anyid],
		[
			OZ_URL,
			language === "en"
				? oz
						.replaceAll("현재 페이지", "Current page")
						.replaceAll("한 페이지 전으로 이동", "Move to the previous page")
						.replaceAll("한 페이지 다음으로 이동", "Move to the next page")
				: oz,
		],
	]);
	const unexpected: string[] = [];
	await context.route("**/*", async (route) => {
		const url = route.request().url();
		const body = responses.get(url);
		if (body === undefined) {
			unexpected.push(url);
			await route.abort("blockedbyclient");
			return;
		}
		await route.fulfill({
			status: 200,
			contentType: "text/html; charset=utf-8",
			body,
		});
	});
	return unexpected;
}

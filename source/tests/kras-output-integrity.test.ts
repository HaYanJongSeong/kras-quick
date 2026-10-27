import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { it } from "node:test";
import {
	isKrasStage3OutputComplete,
	propertyAutoStagePaths,
	runKrasAutoStage3,
} from "../scripts/kras-auto-stages.ts";
import { TEST_PDF, TEST_PNG, TEST_XML } from "./kras-capture-fixture.ts";

const building = { index: 0, value: "", label: "" };
const stage2 = {
	version: 1,
	stage: 2,
	status: "completed",
	address: "서울특별시 용산구 용산동2가 5-227",
	building,
} as const;
const surface = {
	url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
	hasVisibleCaptcha: async () => false,
	capturePdf: async () => TEST_PDF,
	capturePagePngs: async () => [TEST_PNG, TEST_PNG],
	readReportXml: async () => TEST_XML,
};

it("rejects empty artifacts when resuming", async () => {
	// Given
	const root = await mkdtemp(join(tmpdir(), "kras-integrity-"));
	const paths = propertyAutoStagePaths(
		"서울특별시 용산구 용산동2가 5-227",
		root,
	);
	try {
		await mkdir(paths.otherDirectory, { recursive: true });
		for (const file of [
			paths.pdf,
			paths.pagePng(1),
			paths.pageXml,
			paths.structureJson,
		])
			await writeFile(file, "");
		// When / Then
		assert.equal(
			isKrasStage3OutputComplete(paths, { index: 0, value: "", label: "" }),
			false,
		);
	} finally {
		await rm(root, { recursive: true, force: true });
	}
});

for (const damage of [
	"missing-second",
	"empty-second",
	"tampered-second",
	"pdf",
	"xml",
	"empty-json",
	"legacy",
	"metadata",
	"structure",
	"target",
	"outside",
] as const) {
	it(`rejects ${damage} output after a successful cached verification`, async () => {
		// Given
		const root = await mkdtemp(join(tmpdir(), "kras-integrity-"));
		const paths = propertyAutoStagePaths(stage2.address, root);
		try {
			await runKrasAutoStage3(stage2, surface, paths);
			assert.equal(isKrasStage3OutputComplete(paths, building), true);
			assert.equal(isKrasStage3OutputComplete(paths, building), true);
			// When
			switch (damage) {
				case "missing-second":
					await rm(paths.pagePng(2));
					break;
				case "empty-second":
					await writeFile(paths.pagePng(2), "");
					break;
				case "tampered-second": {
					const png = Buffer.from(TEST_PNG);
					png[40] = (png[40] ?? 0) ^ 1;
					await writeFile(paths.pagePng(2), png);
					break;
				}
				case "pdf":
					await writeFile(paths.pdf, "bad-pdf");
					break;
				case "xml":
					await writeFile(paths.pageXml, TEST_XML.replace("용산", "종로"));
					break;
				case "empty-json":
					await writeFile(paths.structureJson, "");
					break;
				case "legacy":
					await writeFile(paths.structureJson, "{}");
					break;
				case "metadata":
					await writeFile(
						paths.structureJson,
						'{"_krasIntegrity":{"version":1}}',
					);
					break;
				case "structure":
				case "target":
				case "outside": {
					const json = await readFile(paths.structureJson, "utf8");
					const changed =
						damage === "structure"
							? json.replace('"address":', '"unexpected":')
							: damage === "target"
								? json.replace('"value": ""', '"value": "wrong"')
								: json.replace(
										/"file": "[^"]+"/u,
										'"file": "../../../outside.pdf"',
									);
					await writeFile(paths.structureJson, changed);
					break;
				}
				default:
					throw new Error(`Unexpected damage: ${damage satisfies never}`);
			}
			// Then
			assert.equal(isKrasStage3OutputComplete(paths, building), false);
		} finally {
			await rm(root, { recursive: true, force: true });
		}
	});
}

for (const artifact of ["pdf", "png", "json", "xml"] as const) {
	it(`preserves legacy ${artifact} on automatic save conflict`, async () => {
		// Given
		const root = await mkdtemp(join(tmpdir(), "kras-conflict-"));
		const paths = propertyAutoStagePaths(stage2.address, root);
		try {
			await mkdir(paths.otherDirectory, { recursive: true });
			const file = {
				pdf: paths.pdf,
				png: paths.pagePng(2),
				json: paths.structureJson,
				xml: paths.pageXml,
			}[artifact];
			await writeFile(file, "legacy-sentinel");
			// When
			await assert.rejects(runKrasAutoStage3(stage2, surface, paths), {
				name: "KrasOutputConflictError",
			});
			// Then
			assert.equal(await readFile(file, "utf8"), "legacy-sentinel");
			await assert.rejects(readFile(paths.stage3Json), { code: "ENOENT" });
		} finally {
			await rm(root, { recursive: true, force: true });
		}
	});
}

it("blocks malformed captures before writing completed state", async () => {
	// Given
	const root = await mkdtemp(join(tmpdir(), "kras-malformed-"));
	const paths = propertyAutoStagePaths(stage2.address, root);
	try {
		// When
		const result = await runKrasAutoStage3(
			stage2,
			{ ...surface, capturePagePngs: async () => [Buffer.from("png")] },
			paths,
		);
		// Then
		assert.equal(result.status, "blocked");
		await assert.rejects(readFile(paths.pdf), { code: "ENOENT" });
	} finally {
		await rm(root, { recursive: true, force: true });
	}
});

it("preserves a file created after conflict checks and before writes", async () => {
	// Given
	const root = await mkdtemp(join(tmpdir(), "kras-race-"));
	const paths = propertyAutoStagePaths(stage2.address, root);
	try {
		await mkdir(paths.otherDirectory, { recursive: true });
		await writeFile(paths.pageXml, "old-xml");
		// When
		await assert.rejects(
			runKrasAutoStage3(stage2, surface, paths, async () => {
				await writeFile(paths.pdf, "concurrent-pdf", { flag: "wx" });
				return "-new";
			}),
			{ code: "EEXIST" },
		);
		// Then
		assert.equal(await readFile(paths.pdf, "utf8"), "concurrent-pdf");
		assert.equal(await readFile(paths.pageXml, "utf8"), "old-xml");
		await assert.rejects(readFile(paths.stage3Json), { code: "ENOENT" });
	} finally {
		await rm(root, { recursive: true, force: true });
	}
});

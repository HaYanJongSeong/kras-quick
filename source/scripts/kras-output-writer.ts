import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { basename, extname, join } from "node:path";
import type {
	KrasAutoStage3Output,
	KrasAutoStagePaths,
	KrasOzCapture,
} from "./kras-auto-stages.ts";
import { createOZStructure, parseKrasXml } from "./kras-excel-updater.ts";
import {
	describeArtifact,
	type KrasOutputTarget,
} from "./kras-output-integrity.ts";

export function outputLabel(target: KrasOutputTarget): string {
	return [target.building.label, target.floorRoom?.label]
		.filter(Boolean)
		.join("-")
		.trim()
		.replace(/[\\/:*?"<>|]/gu, "")
		.replace(/[. ]+$/gu, "");
}

export function withBuildingSuffix(
	path: string,
	directory: string,
	buildingLabel: string,
): string {
	return buildingLabel === ""
		? path
		: join(
				directory,
				`${basename(path, extname(path))}-${buildingLabel}${extname(path)}`,
			);
}

export class KrasOutputConflictError extends Error {
	readonly path: string;
	constructor(path: string) {
		super(`기존 산출물을 보존하기 위해 저장을 중단합니다: ${path}`);
		this.path = path;
		this.name = "KrasOutputConflictError";
	}
}

type WriteDestination = { readonly path: string; readonly flag: "w" | "wx" };
async function resolveDestination(
	path: string,
	resolveConflict?: (path: string) => Promise<string>,
): Promise<WriteDestination> {
	if (!existsSync(path)) return { path, flag: "wx" };
	if (resolveConflict === undefined) throw new KrasOutputConflictError(path);
	const suffix = (await resolveConflict(path)).trim();
	if (suffix === "") return { path, flag: "w" };
	// biome-ignore lint/suspicious/noControlCharactersInRegex: Windows filenames reject ASCII controls.
	if (/[\u0000-\u001f<>:"/\\|?*]/u.test(suffix))
		throw new Error("파일명 접미사에 사용할 수 없는 문자가 있습니다.");
	const extension = extname(path);
	const candidate = `${path.slice(0, -extension.length)}${suffix.startsWith("-") ? suffix : `-${suffix}`}${extension}`;
	if (existsSync(candidate)) throw new KrasOutputConflictError(candidate);
	return { path: candidate, flag: "wx" };
}

export type KrasOutputSave = {
	readonly paths: KrasAutoStagePaths;
	readonly target: KrasOutputTarget;
	readonly resolveConflict?: ((path: string) => Promise<string>) | undefined;
};

export async function saveKrasOutput(
	capture: KrasOzCapture,
	save: KrasOutputSave,
): Promise<KrasAutoStage3Output> {
	const { paths, target, resolveConflict } = save;
	const label = outputLabel(target);
	const pdf = await resolveDestination(
		withBuildingSuffix(paths.pdf, paths.directory, label),
		resolveConflict,
	);
	const xml = await resolveDestination(
		withBuildingSuffix(paths.pageXml, paths.otherDirectory, label),
		resolveConflict,
	);
	const structure = await resolveDestination(
		withBuildingSuffix(paths.structureJson, paths.otherDirectory, label),
		resolveConflict,
	);
	const pngs: WriteDestination[] = [];
	for (const index of capture.pagePngs.keys())
		pngs.push(
			await resolveDestination(
				withBuildingSuffix(
					paths.pagePng(index + 1),
					paths.otherDirectory,
					label,
				),
				resolveConflict,
			),
		);
	const xmlBytes = Buffer.from(capture.pageXml, "utf8");
	const identity = (option: KrasOutputTarget["building"]) => ({
		value: option.value,
		label: option.label,
	});
	const metadata = {
		version: 1,
		building: identity(target.building),
		floorRoom:
			target.floorRoom === undefined ? null : identity(target.floorRoom),
		pdf: describeArtifact(structure.path, pdf.path, capture.pdf),
		pagePngs: pngs.map((png, index) => {
			const bytes = capture.pagePngs[index];
			if (bytes === undefined) throw new Error("Captured PNG missing");
			return describeArtifact(structure.path, png.path, bytes);
		}),
		pageXml: describeArtifact(structure.path, xml.path, xmlBytes),
	};
	await mkdir(paths.directory, { recursive: true });
	await mkdir(paths.otherDirectory, { recursive: true });
	await writeFile(pdf.path, capture.pdf, { flag: pdf.flag });
	for (const [index, png] of pngs.entries()) {
		const bytes = capture.pagePngs[index];
		if (bytes === undefined) throw new Error("Captured PNG missing");
		await writeFile(png.path, bytes, { flag: png.flag });
	}
	await writeFile(xml.path, xmlBytes, { flag: xml.flag });
	await writeFile(
		structure.path,
		`${JSON.stringify({ ...createOZStructure(parseKrasXml(capture.pageXml)), _krasIntegrity: metadata }, null, 2)}\n`,
		{ flag: structure.flag },
	);
	return {
		pdf: pdf.path,
		pagePngs: pngs.map((png) => png.path),
		pageXml: xml.path,
		structureJson: structure.path,
	};
}

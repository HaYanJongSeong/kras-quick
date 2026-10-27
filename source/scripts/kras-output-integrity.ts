import { createHash } from "node:crypto";
import { lstatSync, readFileSync, realpathSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import type { KrasAutoStagePaths, KrasOzCapture } from "./kras-auto-stages.ts";
import type { KrasBuildingOption } from "./kras-domain.ts";
import { createOZStructure, parseKrasXml } from "./kras-excel-updater.ts";

const identitySchema = z.object({ value: z.string(), label: z.string() });
const artifactSchema = z.object({
	file: z.string().min(1),
	bytes: z.number().int().positive(),
	sha256: z.string().regex(/^[a-f0-9]{64}$/u),
});
const integritySchema = z.object({
	version: z.literal(1),
	building: identitySchema,
	floorRoom: identitySchema.nullable(),
	pdf: artifactSchema,
	pagePngs: z.array(artifactSchema).min(1),
	pageXml: artifactSchema,
});
const structureSchema = z
	.object({ _krasIntegrity: integritySchema })
	.passthrough();
export type KrasOutputTarget = {
	readonly building: KrasBuildingOption;
	readonly floorRoom?: KrasBuildingOption | undefined;
};
type SavedArtifact = z.infer<typeof artifactSchema>;
type CacheEntry = {
	readonly files: readonly string[];
	readonly fingerprints: string;
	readonly target: string;
};
const verifiedOutputs = new WeakMap<
	KrasAutoStagePaths,
	Map<string, CacheEntry>
>();

export function validPdf(bytes: Uint8Array): boolean {
	const buffer = Buffer.from(bytes);
	return (
		buffer.subarray(0, 5).toString() === "%PDF-" &&
		/%%EOF\s*$/u.test(buffer.subarray(-1024).toString())
	);
}

export function validPng(bytes: Uint8Array): boolean {
	const buffer = Buffer.from(bytes);
	return (
		buffer.length >= 45 &&
		buffer
			.subarray(0, 8)
			.equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])) &&
		buffer.readUInt32BE(8) === 13 &&
		buffer.toString("ascii", 12, 16) === "IHDR" &&
		buffer.readUInt32BE(16) > 0 &&
		buffer.readUInt32BE(20) > 0 &&
		buffer
			.subarray(-12)
			.equals(Buffer.from([0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]))
	);
}

export function validCapture(capture: KrasOzCapture): boolean {
	return (
		validPdf(capture.pdf) &&
		capture.pagePngs.length > 0 &&
		capture.pagePngs.every(validPng) &&
		parseKrasXml(capture.pageXml).landLocNm !== ""
	);
}

export function describeArtifact(
	structurePath: string,
	file: string,
	bytes: Uint8Array,
): SavedArtifact {
	return {
		file: relative(dirname(structurePath), file),
		bytes: bytes.byteLength,
		sha256: createHash("sha256").update(bytes).digest("hex"),
	};
}

function containedPath(root: string, file: string): string {
	const path = resolve(file);
	const inside = relative(realpathSync(root), realpathSync(path));
	if (inside === ".." || inside.startsWith(`..${sep}`) || isAbsolute(inside))
		throw new Error("Output path escapes its directory");
	return path;
}

function fingerprints(files: readonly string[]): string {
	return files
		.map((file) => {
			const stat = lstatSync(file, { bigint: true });
			if (!stat.isFile()) throw new Error("Output must be a regular file");
			return [stat.dev, stat.ino, stat.size, stat.mtimeNs, stat.ctimeNs].join(
				":",
			);
		})
		.join("|");
}

export function verifyKrasOutput(
	paths: KrasAutoStagePaths,
	structurePath: string,
	target: KrasOutputTarget,
): boolean {
	try {
		containedPath(paths.directory, structurePath);
		const identity = JSON.stringify([
			identitySchema.parse(target.building),
			target.floorRoom === undefined
				? null
				: identitySchema.parse(target.floorRoom),
		]);
		const cache = verifiedOutputs.get(paths);
		const cached = cache?.get(structurePath);
		if (
			cached !== undefined &&
			cached.target === identity &&
			fingerprints(cached.files) === cached.fingerprints
		)
			return true;
		const structure = structureSchema.parse(
			JSON.parse(readFileSync(structurePath, "utf8")),
		);
		const integrity = structure._krasIntegrity;
		if (JSON.stringify([integrity.building, integrity.floorRoom]) !== identity)
			return false;
		const artifacts = [integrity.pdf, ...integrity.pagePngs, integrity.pageXml];
		const files = artifacts.map((artifact) => {
			if (isAbsolute(artifact.file))
				throw new Error("Artifact paths must be relative");
			return containedPath(
				paths.directory,
				resolve(dirname(structurePath), artifact.file),
			);
		});
		if (
			new Set(files).size !== files.length ||
			files.includes(resolve(structurePath))
		)
			return false;
		const watched = [structurePath, ...files];
		const before = fingerprints(watched);
		const bytes = files.map((file) => readFileSync(file));
		for (const [index, artifact] of artifacts.entries()) {
			const data = bytes[index];
			if (
				data === undefined ||
				data.length !== artifact.bytes ||
				createHash("sha256").update(data).digest("hex") !== artifact.sha256
			)
				return false;
		}
		const pdf = bytes[0];
		const xml = bytes.at(-1)?.toString("utf8");
		if (
			pdf === undefined ||
			xml === undefined ||
			!validCapture({ pdf, pagePngs: bytes.slice(1, -1), pageXml: xml })
		)
			return false;
		const { _krasIntegrity: _metadata, ...savedStructure } = structure;
		if (
			!isDeepStrictEqual(
				savedStructure,
				createOZStructure(parseKrasXml(xml)),
			) ||
			fingerprints(watched) !== before
		)
			return false;
		const entries = cache ?? new Map<string, CacheEntry>();
		entries.set(structurePath, {
			files: watched,
			fingerprints: before,
			target: identity,
		});
		verifiedOutputs.set(paths, entries);
		return true;
	} catch (error) {
		if (error instanceof Error) return false;
		throw error;
	}
}

import { PDFDocument } from "pdf-lib";

const document = await PDFDocument.create();
document.addPage([64, 48]);
export const TEST_PDF = Buffer.from(await document.save());
export const TEST_PNG = Buffer.from(
	"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=",
	"base64",
);
export const TEST_XML =
	"<REAL_LAND_BLDG><LAND_LOC_NM>서울특별시 용산구 용산동2가 5-227</LAND_LOC_NM></REAL_LAND_BLDG>";

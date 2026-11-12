import type { Page } from "playwright-core";
import { isKrasTargetUrl } from "./kras-domain.ts";
import { KrasLookupTargetError } from "./kras-page.ts";

export async function lookupAfterNavigation<T>(
	page: Pick<Page, "waitForLoadState" | "isClosed" | "url">,
	lookup: () => Promise<T>,
): Promise<T> {
	for (const attempt of [1, 2, 3] as const) {
		await page.waitForLoadState("domcontentloaded", { timeout: 15_000 });
		if (!isKrasTargetUrl(page.url())) throw new KrasLookupTargetError();
		try {
			return await lookup();
		} catch (error: unknown) {
			if (
				!(error instanceof Error) ||
				!/Execution context was destroyed|Cannot find context with specified id/.test(
					error.message,
				) ||
				page.isClosed() ||
				attempt === 3
			)
				throw error;
		}
	}
	throw new KrasLookupTargetError();
}

export type AutoAddressFailure = {
	readonly input: string;
	readonly reason: string;
};

export function formatAutoFailures(
	failures: ReadonlyMap<string, AutoAddressFailure>,
): string {
	if (failures.size === 0) return "";
	return [
		`처리 실패 ${failures.size}건 (완료되지 않은 주소, 재확인 필요)`,
		...Array.from(failures, ([address, failure]) =>
			[
				`- ${address}`,
				...(failure.input === address ? [] : [`  원본 입력: ${failure.input}`]),
				`  사유: ${failure.reason.replace(/\s+/g, " ").trim()}`,
			].join("\n"),
		),
		"",
	].join("\n");
}

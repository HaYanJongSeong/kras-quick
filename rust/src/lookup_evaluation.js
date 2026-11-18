(async function lookupEvaluation(
	request                       ,
)                                {
	const {
		address: rawAddress,
		darkMode: useDarkMode,
		darkModeCss,
		hasLegalRi,
		ledgerLabel,
	} = request;
	class KrasEvaluationError extends Error {
		                  name = "KrasEvaluationError";
	}

	const normalize = (value               )         =>
		(value ?? "").replace(/\s+/g, " ").trim();
	const assertNeverMode = (unhandled       )        => {
		throw new KrasEvaluationError(
			`처리할 수 없는 KRAS 평가 모드: ${String(unhandled)}`,
		);
	};
	const normalizedAddress = normalize(rawAddress);
	const requiredSelect = (
		selector        ,
		field        ,
	)                    => {
		const select = document.querySelector                   (selector);
		if (select === null)
			throw new KrasEvaluationError(`${field} 필드를 찾지 못했습니다.`);
		return select;
	};
	const matchingAddressOption = (
		select                   ,
	)                                =>
		Array.from(select.options)
			.filter((option) => {
				const label = normalize(option.textContent);
				return (
					label !== "" &&
					label !== "선택" &&
					label !== "선택사항없음" &&
					normalizedAddress.includes(label)
				);
			})
			.sort(
				(left, right) =>
					normalize(right.textContent).length -
					normalize(left.textContent).length,
			)[0];
	const placeholderOption = (
		select                   ,
	)                                =>
		Array.from(select.options).find((option) => {
			const label = normalize(option.textContent);
			return label === "" || label === "선택" || label === "선택사항없음";
		});
	const optionFingerprint = (select                   )         =>
		JSON.stringify(
			Array.from(select.options).map((option) => [
				option.value,
				normalize(option.textContent),
			]),
		);
	                     
		                          
	                                   
	                           
		                                    
		                                           
		                              
	  
	const observeRefresh = (
		select                   ,
		expectation                    ,
	) => {
		let cancel = ()       => undefined;
		const done = new Promise      ((resolve, reject) => {
			let settled = false;
			let quietId                    ;
			let timeoutId                    ;
			const observer = new MutationObserver(() => {
				if (settled) return;
				if (quietId !== undefined) window.clearTimeout(quietId);
				quietId = window.setTimeout(() => {
					quietId = undefined;
					const fingerprintChanged =
						optionFingerprint(select) !== expectation.initialFingerprint;
					if (
						settled ||
						expectation.target(select) === undefined ||
						(expectation.requireFingerprintChange && !fingerprintChanged)
					) {
						return;
					}
					settled = true;
					observer.disconnect();
					if (timeoutId !== undefined) window.clearTimeout(timeoutId);
					resolve();
				}, 100);
			});
			observer.observe(select, { childList: true, subtree: true });
			timeoutId = window.setTimeout(() => {
				if (settled) return;
				settled = true;
				observer.disconnect();
				if (quietId !== undefined) window.clearTimeout(quietId);
				reject(
					new KrasEvaluationError("종속 주소 선택지가 갱신되지 않았습니다."),
				);
			}, 4000);
			cancel = () => {
				if (settled) return;
				settled = true;
				observer.disconnect();
				if (quietId !== undefined) window.clearTimeout(quietId);
				if (timeoutId !== undefined) window.clearTimeout(timeoutId);
			};
		});
		return { done, cancel: () => cancel() };
	};
	const selectedFields         
		                                   
		                       
	   = [];
	const applyOption = (
		select                   ,
		option                   ,
		label        ,
	)       => {
		select.value = option.value;
		select.dispatchEvent(new Event("input", { bubbles: true }));
		select.dispatchEvent(new Event("change", { bubbles: true }));
		selectedFields.push({ select, label });
	};
	const selectAddress = (selector        , field        )       => {
		const select = requiredSelect(selector, field);
		const match = matchingAddressOption(select);
		if (match === undefined) {
			throw new KrasEvaluationError(`${field} 주소를 선택하지 못했습니다.`);
		}
		applyOption(select, match, normalize(match.textContent));
	};
	const selectPlaceholder = (selector        )       => {
		const select = document.querySelector                   (selector);
		if (select === null) return;
		const placeholder = placeholderOption(select);
		if (placeholder === undefined) {
			throw new KrasEvaluationError(
				`${selector} 기본 선택지를 찾지 못했습니다.`,
			);
		}
		applyOption(select, placeholder, normalize(placeholder.textContent));
	};
	const exactOption = (select                   , label        )       => {
		const option = Array.from(select.options).find(
			(candidate) => normalize(candidate.textContent) === label,
		);
		if (option === undefined) {
			throw new KrasEvaluationError(`지목 ${label}을 선택하지 못했습니다.`);
		}
		applyOption(select, option, label);
	};
	                    
		                                        
		                                   
		                             
		                                
		                                           
		                              
	  
	const selectParent = async (step             )                => {
		const parent = requiredSelect(step.parentSelector, step.parentField);
		const option = matchingAddressOption(parent);
		if (option === undefined) {
			throw new KrasEvaluationError(
				`${step.parentField} 주소를 선택하지 못했습니다.`,
			);
		}
		const label = normalize(option.textContent);
		if (parent.value === option.value) {
			selectedFields.push({ select: parent, label });
			return;
		}
		if (!step.awaitDependentRefresh) {
			applyOption(parent, option, label);
			return;
		}
		const dependent = requiredSelect(
			step.dependentSelector,
			step.dependentSelector,
		);
		const refresh = observeRefresh(dependent, {
			initialFingerprint: optionFingerprint(dependent),
			requireFingerprintChange: step.requireFingerprintChange,
			target: step.target,
		});
		try {
			applyOption(parent, option, label);
			await refresh.done;
		} finally {
			refresh.cancel();
		}
	};

	try {
		if (useDarkMode) {
			const existing = document.getElementById("opencode-dark-mode");
			const style = existing ?? document.createElement("style");
			style.id = "opencode-dark-mode";
			style.textContent = darkModeCss;
			if (existing === null) document.head.append(style);
		}

		const numberMatch = normalizedAddress.match(
			/(?:^|\s)(산\s*)?(\d+)(?:-(\d+))?$/,
		);
		const mainNumber = numberMatch?.[2];
		if (mainNumber === undefined)
			throw new KrasEvaluationError("본번을 확인하지 못했습니다.");
		const subNumber = numberMatch?.[3] ?? "";
		await selectParent({
			awaitDependentRefresh: true,
			dependentSelector: "#sggCd",
			parentField: "시도",
			parentSelector: "#ctpvCd",
			requireFingerprintChange: true,
			target: matchingAddressOption,
		});
		await selectParent({
			awaitDependentRefresh: true,
			dependentSelector: "#emdCd",
			parentField: "시군구",
			parentSelector: "#sggCd",
			requireFingerprintChange: true,
			target: matchingAddressOption,
		});
		await selectParent({
			awaitDependentRefresh: hasLegalRi,
			dependentSelector: "#riCd",
			parentField: "읍면동",
			parentSelector: "#emdCd",
			requireFingerprintChange: hasLegalRi,
			target: hasLegalRi ? matchingAddressOption : placeholderOption,
		});
		if (hasLegalRi) {
			selectAddress("#riCd", "리");
		} else {
			selectPlaceholder("#riCd");
		}

		const landType = requiredSelect("#ldgrSeCd", "지목");
		exactOption(landType, ledgerLabel);

		const mainInput = document.querySelector                  ("#mno");
		const subInput = document.querySelector                  ("#sno");
		if (mainInput === null || subInput === null) {
			throw new KrasEvaluationError("번지 필드를 찾지 못했습니다.");
		}
		for (const [input, value] of [
			[mainInput, mainNumber],
			[subInput, subNumber],
		]         ) {
			input.value = value;
			input.dispatchEvent(new Event("input", { bubbles: true }));
			input.dispatchEvent(new Event("change", { bubbles: true }));
		}
		for (const field of selectedFields) {
			if (
				normalize(field.select.selectedOptions[0]?.textContent ?? null) !==
				field.label
			) {
				throw new KrasEvaluationError(
					`${field.label} 선택값 검증에 실패했습니다.`,
				);
			}
		}
		if (mainInput.value !== mainNumber || subInput.value !== subNumber) {
			throw new KrasEvaluationError("번지 입력값 검증에 실패했습니다.");
		}

		switch (request.mode) {
			case "fill_only":
				return { kind: "fill_only" };
			case "lookup":
				break;
			default:
				return assertNeverMode(request);
		}

		const buttons = Array.from(
			document.querySelectorAll             (
				"button,input[type='button'],input[type='submit']",
			),
		).filter((element) => {
			const label =
				element instanceof HTMLInputElement
					? element.value.trim()
					: normalize(element.textContent);
			const enabled =
				element instanceof HTMLButtonElement ||
				element instanceof HTMLInputElement
					? !element.disabled
					: true;
			return enabled && label === "조회";
		});
		if (buttons.length !== 1) {
			throw new KrasEvaluationError("조회 버튼은 정확히 하나여야 합니다.");
		}
		const button = buttons[0];
		if (button === undefined || typeof certView === "undefined") {
			throw new KrasEvaluationError("조회 실행 함수를 찾지 못했습니다.");
		}

		let complete = ()       => {
			throw new KrasEvaluationError("조회 완료 신호가 준비되지 않았습니다.");
		};
		let emptyRequestInfoCalled = false;
		const completion = new Promise      ((resolve) => {
			complete = resolve;
		});
		const originalCheckLedger = certView.fnCheckLedger;
		const originalEmptyRequest = certView.fnEmptyRequestInfo;
		certView.fnCheckLedger = function wrappedCheckLedger(...args)          {
			try {
				return originalCheckLedger.apply(this, args);
			} finally {
				complete();
			}
		};
		certView.fnEmptyRequestInfo = function wrappedEmptyRequest(
			...args
		)          {
			emptyRequestInfoCalled = true;
			try {
				return originalEmptyRequest.apply(this, args);
			} finally {
				complete();
			}
		};
		let timeoutId                    ;
		let noticeTimer;
		const noticeFailure = new Promise((_resolve, reject) => {
			noticeTimer = window.setInterval(() => {
				const dialog = document.querySelector('.swal2-container.swal2-backdrop-show');
				const text = (dialog?.textContent ?? '').replace(/\s+/g, '');
				if (/오류가발생하였습니다|차단|접속제한|접근제한|로그인.{0,8}만료/.test(text)) {
					reject(new KrasEvaluationError('조회 오류 안내 감지'));
				}
			}, 200);
		});
		const timeout = new Promise       ((_resolve, reject) => {
			timeoutId = window.setTimeout(
				() => reject(new KrasEvaluationError("조회 완료 시간 초과")),
				300_000,
			);
		});
		try {
			button.click();
			await Promise.race([completion, timeout, noticeFailure]);
		} finally {
			certView.fnCheckLedger = originalCheckLedger;
			certView.fnEmptyRequestInfo = originalEmptyRequest;
			if (timeoutId !== undefined) window.clearTimeout(timeoutId);
			if (noticeTimer !== undefined) window.clearInterval(noticeTimer);
		}

		const readBuildingOptions = ()                       => {
			const building = requiredSelect("#bldgSeCd", "건물 구분");
			return Array.from(building.options)
				.map((option, index) => ({
					index,
					value: option.value,
					label: normalize(option.textContent),
				}))
				.filter(
					({ label, value }) =>
						value !== "" &&
						label !== "" &&
						label !== "선택" &&
						label !== "선택사항없음",
				);
		};
		const dismissNoParcelDialog = ()       => {
			if (!emptyRequestInfoCalled) return;
			const visible = (element         )          => {
				const html = element               ;
				return html.offsetParent !== null;
			};
			for (const dialog of Array.from(
				document.querySelectorAll             ('[role="dialog"]'),
			)) {
				if (!visible(dialog)) continue;
				const ok = Array.from(
					dialog.querySelectorAll             (
						'button,input[type="button"],input[type="submit"]',
					),
				).find((element) => {
					const label =
						element instanceof HTMLInputElement
							? element.value
							: element.textContent;
					return normalize(label) === "OK";
				});
				if (ok !== undefined) {
					ok.click();
					return;
				}
			}
		};
		let previousFingerprint = "";
		let stableSince = Date.now();
		const deadline = Date.now() + 30_000;
		const ledgerState = certView                
			                                                          
		 ;
		const expectedBuildingCount = Array.isArray(ledgerState.ledger?.child)
			? ledgerState.ledger.child.length
			: undefined;
		while (Date.now() < deadline) {
			const buildingOptions = readBuildingOptions();
			const fingerprint = JSON.stringify(buildingOptions);
			if (fingerprint !== previousFingerprint) {
				previousFingerprint = fingerprint;
				stableSince = Date.now();
			}
			const emptyPlaceholder =
				document.querySelector                   ("#bldgSeCd")?.options
					.length === 1 &&
				normalize(
					document.querySelector                   ("#bldgSeCd")?.options[0]
						?.textContent ?? null,
				) === "선택사항없음";
			const countComplete =
				expectedBuildingCount === undefined ||
				buildingOptions.length >= expectedBuildingCount;
			if (
				Date.now() - stableSince >= 800 &&
				countComplete &&
				(buildingOptions.length > 0 || emptyPlaceholder)
			) {
				const hasBuilding = buildingOptions.length > 0;
				dismissNoParcelDialog();
				return {
					kind: "lookup",
					hasBuilding,
					buildingOptions,
					...(emptyRequestInfoCalled ? { noParcel: true } : {}),
				};
			}
			await new Promise((resolve) => window.setTimeout(resolve, 100));
		}
		const buildingOptions = readBuildingOptions();
		if (
			expectedBuildingCount !== undefined &&
			buildingOptions.length < expectedBuildingCount
		) {
			throw new KrasEvaluationError(
				"건물구분 목록이 끝까지 로딩되지 않았습니다.",
			);
		}
		const hasBuilding = buildingOptions.length > 0;
		dismissNoParcelDialog();
		return {
			kind: "lookup",
			hasBuilding,
			buildingOptions,
			...(emptyRequestInfoCalled ? { noParcel: true } : {}),
		};
	} catch (error) {
		if (error instanceof KrasEvaluationError) {
			return { kind: "evaluation_error", code: "KRAS_EVALUATION_ERROR" };
		}
		throw error;
	}
})

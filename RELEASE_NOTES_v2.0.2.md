# KRAS Quick v2.0.2

## 변경 사항

- AUTO `일시정지`/`재개` 추가
- 로그인 세션 만료 시 stale CAPTCHA 재시도 중단
- 도호 불일치·출력축척 안내창 자동 확인
- CAPTCHA OCR 실패 후 수동 입력 전환 유지
- 런타임 및 EXE 버전 `2.0.2` 갱신

## 설치

1. `kras-quick.exe`를 다운로드합니다.
2. Google Chrome을 설치합니다.
3. EXE를 실행합니다.
4. 최초 실행 시 Enter를 눌러 Chrome을 자동 실행하거나, 이미 CDP Chrome을 열었다면 `0`을 입력합니다.

EXE와 같은 폴더에 `kras-quick-runtime-v2.0.2.zip`을 두면 런타임을 재다운로드하지 않습니다.

## 사용 중 제어

```text
일시정지 또는 pause: 현재 주소 완료 후 정지
재개 또는 resume: 다음 주소부터 계속
```

## 검증

- `npm run test:property`: 177 passed
- KRAS 관련 기능 테스트: 96 passed, 기존 `tools/kras.ts` 누락 테스트 2건 제외

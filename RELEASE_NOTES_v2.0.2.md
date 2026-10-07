# KRAS Quick v2.0.2

## 변경 사항

- AUTO에 `일시정지`와 `재개`를 추가했습니다.
- 로그인 세션이 만료되면 더 이상 유효하지 않은 CAPTCHA를 재시도하지 않습니다.
- 도호 불일치·출력축척 안내창은 자동으로 확인합니다.
- CAPTCHA OCR이 실패하면 기존처럼 수동 입력으로 넘어갑니다.
- 런타임과 EXE 버전을 `2.0.2`로 맞췄습니다.

## 설치

1. `kras-quick.exe`를 다운로드합니다.
2. Google Chrome을 설치합니다.
3. EXE를 실행합니다.
4. Enter를 누르면 Chrome이 열립니다. 이미 CDP Chrome을 열어 둔 경우에는 `0`을 입력합니다.

`kras-quick-runtime-v2.0.2.zip`을 EXE와 같은 폴더에 두면 런타임을 다시 받지 않습니다.

## 사용 중 제어

```text
일시정지 또는 pause: 현재 주소 완료 후 정지
재개 또는 resume: 다음 주소부터 계속
```

## 검증

- `npm run test:property`: 177 passed
- KRAS 관련 기능 테스트: 96 passed, 기존 `tools/kras.ts` 누락 테스트 2건 제외

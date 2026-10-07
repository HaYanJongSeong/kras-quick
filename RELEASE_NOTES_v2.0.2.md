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

- `npm run test:property`: 177건 통과
- `npm run test:kras`: 144건 통과
- `npm run typecheck`: 통과
- 설치 테스트: 체크섬 불일치 차단, 버전이 포함된 runtime ZIP 설치, 기존 프로필 보존 확인
- EXE 시작 화면, 런타임 압축 해제·모듈 로딩, Tesseract 실행·캐시 재사용 확인

## 같은 버전 수정본 (2026-10-07)

버전은 `2.0.2`로 유지했습니다. AUTO 일시정지·재개에서 터미널 인자가 빠진 오류, npm 실행기의 다운로드 파일명, GitHub 소스 빌드 경로를 고쳤습니다. 수정 EXE와 runtime ZIP은 아래 첨부 파일로 다시 받으세요.

npm registry의 기존 `2.0.2`는 덮어쓸 수 없습니다. npm을 쓰려면 GitHub 수정본을 설치하세요.

```powershell
npm install -g https://github.com/HaYanJongSeong/kras-quick/archive/refs/heads/main.tar.gz
kras-quick
```

실제 로그인 계정으로 KRAS 조회부터 OZ PDF 저장까지 하는 현장 검증은 아직 하지 않았습니다.

# KRAS Quick v2.1.0

KRAS/OZ 토지·건축물 조회 자동화 도구입니다.

## GitHub 설치

PowerShell에 아래 명령을 붙여 넣으면 설치가 시작됩니다. `git`, `gh`, Node.js는 따로 설치하지 않아도 됩니다.

```powershell
irm https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/install.ps1 | iex
```

설치 스크립트는 GitHub Release `v2.1.0`에서 EXE와 runtime ZIP을 받습니다. 각 파일의 SHA-256을 확인한 뒤 `Downloads\kras-quick.exe`를 설치하고 실행합니다.

## 수동 설치

[v2.1.0 Release](https://github.com/HaYanJongSeong/kras-quick/releases/tag/v2.1.0)에서 다음 파일을 같은 폴더에 다운로드합니다.

```text
kras-quick.exe
kras-quick-runtime-v2.1.0.zip
```

두 파일을 받은 뒤 `kras-quick.exe`를 실행하세요.

## npm으로 설치

npm registry의 기존 공개 버전은 `2.0.3`입니다. 이 릴리스에서는 npm registry에 게시하지 않습니다. **2.1.0은 위 GitHub 설치 또는 수동 설치를 사용하세요.** 아래 명령은 기존 registry 버전을 설치합니다.

```powershell
npm install -g @hayanjongseong/kras-quick@latest
kras-quick
```

설치 없이 실행하려면 다음 명령을 쓰세요.

```powershell
npx --yes @hayanjongseong/kras-quick@latest
```

npm·npx는 Node.js 18 이상이 필요합니다. `2.0.2` registry 배포본에는 다운로드 주소 오류가 남아 있으므로 버전을 고정해 쓰고 있다면 `2.0.3`으로 바꾸세요.

## 요구 사항

- Windows 10 이상
- 설치된 Chrome, Edge, Brave, Vivaldi, Opera 또는 Whale. Firefox는 시험 지원입니다.
- 인터넷 연결(최초 runtime 다운로드 또는 KRAS 조회)

프로그램에서 사용할 브라우저를 선택합니다. Chromium 계열은 업무용 별도 프로필과 CDP 연결을 사용합니다. Firefox는 한국어로 실행되며, 프로그램 종료 시 Quick이 연 Firefox 창도 닫힙니다. 기존 개인 프로필은 사용하지 않습니다.

## 2.1.0 변경 사항

- 단일 필지 AUTO에서도 대규모 모드와 같은 처리 범위를 선택합니다: 토지만, 토지+일반건물, 토지+일반건물+집합건물. Enter 기본값은 전체입니다.
- Firefox OZ 도구 모음의 한국어·영어 페이지 이동 선택자를 지원합니다.
- 현재 작업에 연결된 OZ 보고서를 캡처하도록 포터블본을 최신 소스와 맞췄습니다.
- 한글 경로의 Windows 프로세스 식별 시 PowerShell 출력 인코딩을 UTF-8로 지정합니다.

자동검사와 실행기 시작·취소를 검증했습니다. 2.1.0의 실제 로그인·필지 조회·CAPTCHA 처리와 브라우저별 전체 AUTO는 이번 릴리스 검증에서 실행하지 않았습니다.

## 개발

```powershell
npm ci --prefix source
npm run test:property
npm run test:kras
npm run typecheck
```

기능 테스트와 타입 검사는 `source/`의 개발 의존성을 사용합니다.

직접 빌드할 때는 `C:\Program Files\Tesseract-OCR`에 Tesseract를 설치하고 아래 명령을 실행하세요.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-portable.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-single-exe.ps1
```

## 검증

수정본의 검증 결과와 아직 확인하지 못한 항목은 [VERIFICATION.md](VERIFICATION.md)에 기록했습니다.

## 라이선스

[MIT](LICENSE)

# KRAS Quick v2.0.2

KRAS/OZ 토지·건축물 조회 자동화 도구입니다.

## GitHub 설치

PowerShell에 아래 명령을 붙여 넣으면 설치가 시작됩니다. `git`, `gh`, Node.js는 따로 설치하지 않아도 됩니다.

```powershell
irm https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/install.ps1 | iex
```

설치 스크립트는 GitHub Release `v2.0.2`에서 EXE와 runtime ZIP을 받습니다. 각 파일의 SHA-256을 확인한 뒤 `Downloads\kras-quick.exe`를 설치하고 실행합니다.

## 수동 설치

[v2.0.2 Release](https://github.com/HaYanJongSeong/kras-quick/releases/tag/v2.0.2)에서 다음 파일을 같은 폴더에 다운로드합니다.

```text
kras-quick.exe
kras-quick-runtime-v2.0.2.zip
```

두 파일을 받은 뒤 `kras-quick.exe`를 실행하세요.

## npm으로 설치

npm registry에 올라간 `2.0.2`에는 실행기 오류가 있습니다. 이미 게시한 버전은 덮어쓸 수 없어, 버전을 올리지 않고 GitHub 수정본으로 설치하도록 안내합니다. 기존 npm 설치도 아래 명령으로 교체할 수 있습니다.

```powershell
npm install -g https://github.com/HaYanJongSeong/kras-quick/archive/refs/heads/main.tar.gz
kras-quick
```

이 경로는 Node.js 18 이상이 필요합니다. 설치되는 버전은 그대로 `2.0.2`입니다. `npm install -g @hayanjongseong/kras-quick`은 아직 수정 전 registry 배포본을 받으므로 사용하지 마세요.

## 요구 사항

- Windows 10 이상
- Google Chrome
- 인터넷 연결(최초 runtime 다운로드 또는 KRAS 조회)

프로그램을 실행하면 Chrome이 CDP 모드로 열립니다. 기존 Chrome 프로필은 종료하지 않습니다.

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

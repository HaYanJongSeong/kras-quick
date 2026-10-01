# KRAS Quick v2.0.2

KRAS/OZ 토지·건축물 조회 자동화 도구입니다.

## GitHub 설치

PowerShell에서 한 줄만 실행합니다. `git`, `gh`, Node.js가 필요 없습니다.

```powershell
irm https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/install.ps1 | iex
```

설치 스크립트가 GitHub Release `v2.0.2`에서 EXE와 runtime ZIP을 다운로드하고, 각 파일의 SHA-256을 검증한 뒤 `Downloads\kras-quick.exe`를 설치·실행합니다.

## 수동 설치

[v2.0.2 Release](https://github.com/HaYanJongSeong/kras-quick/releases/tag/v2.0.2)에서 다음 파일을 같은 폴더에 다운로드합니다.

```text
kras-quick.exe
kras-quick-runtime-v2.0.2.zip
```

그 뒤 `kras-quick.exe`를 실행합니다.

## 요구 사항

- Windows 10 이상
- Google Chrome
- 인터넷 연결(최초 runtime 다운로드 또는 KRAS 조회)

Chrome은 실행 시 CDP 모드로 자동 시작합니다. 기존 Chrome 프로필은 종료하지 않습니다.

## 개발

```powershell
npm install
npm run test:property
npm run test:kras
npm run typecheck
```

직접 빌드하려면 `C:\Program Files\Tesseract-OCR`에 Tesseract를 설치한 뒤 실행합니다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-portable.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-single-exe.ps1
```

## 라이선스

[MIT](LICENSE)

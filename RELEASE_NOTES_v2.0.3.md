# KRAS Quick v2.0.3

npm·npx 실행기가 찾던 EXE 파일명을 실제 GitHub Release 파일명과 맞췄습니다. BOM 없이 패키징하고 실행기와 runtime ZIP의 SHA-256을 새 빌드에 맞춰 갱신했습니다.

AUTO 일시정지·재개에서 터미널 인자가 빠진 오류도 고쳤습니다. GitHub 소스를 내려받아 빌드할 때의 경로와 설치 스크립트의 runtime ZIP 이름을 정리했습니다.

## 설치

```powershell
npm install -g @hayanjongseong/kras-quick@latest
kras-quick
```

설치 없이 실행:

```powershell
npx --yes @hayanjongseong/kras-quick@latest
```

Node.js 없이 설치:

```powershell
irm https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/install.ps1 | iex
```

수동 설치는 `kras-quick.exe`와 `kras-quick-runtime-v2.0.3.zip`을 같은 폴더에 놓고 EXE를 실행하세요.

## 검증

- KRAS 기능 테스트 144건, property 테스트 177건 통과
- 타입 검사와 실행기 패키징 검사 통과
- 런타임 압축 해제, 전체 모듈 import, Tesseract 실행, 캐시 재사용 확인
- EXE 시작 화면 확인

실제 로그인 계정으로 KRAS 조회부터 OZ PDF 저장까지 진행하는 현장 검증은 아직 하지 않았습니다.

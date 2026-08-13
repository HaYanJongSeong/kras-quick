# kras-quick Release Publisher & Installer

<<<<<<< Updated upstream
## 구조
```
kras-quick-release/
├── .gitignore                      # *.exe / *.sha256 제외
├── README.md
├── install/
│   ├── install-kras-quick.ps1      # 설치기 (gh release download + SHA-256 검증 + 원자적 설치)
│   └── install-kras-quick.cmd      # 설치기 실행 래퍼 (더블클릭용)
├── scripts/
│   └── release-kras-quick.ps1      # 배포기 (체크섬 생성 + 초안 비공개 릴리스 + 자산 업로드)
└── tests/
    └── test-installer.ps1          # 오프라인 자체 테스트 (로컬 스테이징)
```

## 배포 (Publisher)

```powershell
# 초안(draft) 비공개 릴리스 생성 + EXE/체크섬 업로드
scripts\release-kras-quick.ps1 -ExePath C:\Users\admin\Downloads\kras_quick_v0.2.1.exe -Tag v0.2.1

# 릴리스 노트/타이틀 지정, 즉시 공개
scripts\release-kras-quick.ps1 -ExePath ... -Tag v0.2.1 -Title "v0.2.1" -Notes "..." -Publish

# 체크섬 파일(.sha256)만 생성 (gh 없이, 테스트용)
scripts\release-kras-quick.ps1 -ExePath ... -Tag v0.2.1 -StageOnly
```

동작:
1. `Get-FileHash -Algorithm SHA256`로 체크섬 계산 → `<exe>.sha256` 파일 작성 (`<hash>  <파일명>` 형식)
2. `gh release create --draft` 로 **초안** 릴리스 생성 (저장소가 private 이므로 릴리스도 자동 비공개)
3. EXE + `.sha256` 두 자산 업로드 후 `gh release view`로 자산 존재 확인

주의: 저장소가 public이면 `--draft` + `-Publish` 없이 공개되므로, **비공개 유지가 필수면 저장소 접근 권한을 확인**할 것.

## 설치 (Installer)

```powershell
# 최신 릴리스 설치 (기본: C:\Users\<user>\Downloads\kras-quick.exe)
install\install-kras-quick.ps1

# 특정 버전 / 특정 위치
install\install-kras-quick.ps1 -Version v0.2.1 -InstallDir D:\apps
```

또는 `install-kras-quick.cmd` 더블클릭 (인자 그대로 전달).

동작:
1. `gh release download` 로 `.exe` + `.sha256` 자산을 임시 폴더에 내려받음
2. 체크섬 파일의 해시와 실제 EXE 해시를 비교 → 불일치 시 **설치하지 않고** 종료(exit 1)
3. 원자적 설치: `kras-quick.exe.<guid>.tmp` 로 복사 후 `Move-Item -Force`로 교체 (같은 볼륨 rename = 원자적)
4. **대상 EXE 파일 하나만 교체** — 같은 폴더의 `.kras-chrome-profile`, KRAS 출력 폴더 등은 전혀 건드리지 않음 (업데이트 시 프로필 보존)

## 오프라인 테스트 (네트워크/gh 불필요)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tests\test-installer.ps1
```

검증 항목:
- 스테이징 폴더의 가짜 EXE+체크섬으로 설치 성공, 프로필 폴더 보존
- 위변조된 체크섬 → 설치 거부(exit 1), 파일 미생성
- `.cmd` 래퍼 동작 + 종료코드 전파
- 배포기 `-StageOnly` 체크섬 파일 내용/형식
- 배포기 잘못된 입력(비-EXE, 없는 파일) 거부

## 보안

- **토큰을 인자/환경변수로 절대 전달하지 않는다.** 인증은 `gh auth login`으로 저장된 gh 자체 자격증명만 사용.
- 체크섬 불일치 시 설치 중단 → 변조/다운로드 오류 방지.
- 설치 전 임시 파일은 `finally`에서 정리.
- 배포 시 `gh release create` 실패/자산 누락을 `$LASTEXITCODE`와 `gh release view`로 검증.

=======
[![Latest release](https://img.shields.io/github/v/release/HaYanJongSeong/kras-quick?style=flat&label=release)](https://github.com/HaYanJongSeong/kras-quick/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat)](https://opensource.org/licenses/MIT)

> KRAS 부동산종합증명서(일사편리) 조회·캡처를 자동화하는 비공식 Windows CLI 도구.

---

## 📦 설치 (한 줄)

git·gh 없이 PowerShell 하나로 설치하고 실행합니다.

```powershell
$u='https://github.com/HaYanJongSeong/kras-quick/releases/download/v0.2.1/kras_quick_v0.2.1.exe';$d="$env:USERPROFILE\Downloads";Invoke-WebRequest $u -OutFile "$d\kras_quick_v0.2.1.exe";Invoke-WebRequest "$u.sha256" -OutFile "$d\kras_quick_v0.2.1.exe.sha256";$h=((Get-Content "$d\kras_quick_v0.2.1.exe.sha256" -Raw) -split '\s+')[0];$a=(Get-FileHash "$d\kras_quick_v0.2.1.exe" -Algorithm SHA256).Hash;if($h -ne $a){throw 'SHA-256 mismatch'};$t="$d\kras-quick.exe.$([guid]::NewGuid().ToString('N')).tmp";Copy-Item "$d\kras_quick_v0.2.1.exe" $t;Move-Item $t "$d\kras-quick.exe" -Force;Start-Process "$d\kras-quick.exe"
```

> `kras_quick_v0.2.1.exe.sha256` 해시와 일치해야 설치되며, 기존 `kras-quick.exe`만 원자적으로 교체됩니다.

## ▶️ 사용법

`kras-quick.exe` 실행 → Chrome이 KRAS 페이지를 엽니다. 지번 주소 입력 → 건물 선택 → 보안문자 직접 입력 → 저장.

## 📁 출력물

실행 파일 위치(기본 `Downloads`) 아래에 저장됩니다.

- `PDF` — 증명서 전체 병합본
- `PNG` — 페이지별 캡처
- `oz-report-data-<건물명>.xml` — OZ 추출 원본
- `excel-bcbt-data-<건물명>.json` — 엑셀 BC:BT 기록용

> 내부 작업 상태는 별도 JSON으로 관리됩니다.

## ⚠️ 요구사항

- Windows 10 이상
- Chrome 설치 (브라우저 프로필은 `%USERPROFILE%\Downloads\.kras-chrome-profile`에 유지)
- 인터넷 연결

## 🔒 비공식 도구 고지

이 도구는 **KRAS·일사편리·OZ Viewer·Softgraphy·Google·Microsoft**와 **제휴·승인·보증 관계가 없습니다**. 사용으로 인한 모든 책임은 사용자 본인에게 있습니다.

## 📄 라이선스

[MIT](LICENSE) © 2026 Seong

---

> 저장소의 `install/`, `scripts/`, `tests/`는 자체 배포·테스트용이며, 일반 사용자에게는 위 한 줄 명령으로 충분합니다.
>>>>>>> Stashed changes

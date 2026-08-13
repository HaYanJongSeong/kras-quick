# kras-quick Release Publisher & Installer

개인 GitHub 저장소 `HaYanJongSeong/kras-quick`의 비공개 릴리스 배포/설치 스크립트 모음.
EXE는 Git에 절대 커밋하지 않고, GitHub Release 자산으로만 배포한다.

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

## 주의사항

- gh CLI 필요: https://cli.github.com/ (설치 시 `gh auth login`).
- 기존 EXE가 `Downloads\kras_quick_v0.2.1.exe`(버전명)이면, 설치기는 고정 이름 `kras-quick.exe`로 새로 설치한다. 이전 버전 EXE는 남겨두므로 원하면 수동 삭제.
- 이 스크립트는 Git 초기화/커밋/푸시, 릴리스 생성, 업로드를 스스로 하지 않는다. 배포는 반드시 배포기를 실행할 때만.

# 배포 검증 기록

## 2.1.1 로컬 검증본 (2026-10-27, 미공개)

다음은 정식 게시 전 로컬 빌드 검증 시점의 기록이다. 이후 커밋·푸시·게시 상태와 구분한다.

- 사용자에게서 개선된 2.1.0 테스트본의 정상 작동 확인을 받은 뒤 2.1.1로 올렸다. 이 확인을 2.1.1의 새 실제 사이트 조회 검증으로 간주하지 않는다.
- Windows 기능 검사 429/429, 실행기 검사 1/1, 타입검사·전체 린트 통과. PC 반영 후에도 429/429와 타입검사·린트를 다시 통과했다.
- Brave·Firefox의 격리된 MOCK 브라우저 검사 20/20 통과. 다른 미설치 브라우저나 실제 로그인·필지 조회·CAPTCHA·전체 AUTO는 새로 실행하지 않았다.
- 소스·포터블·실제 ZIP 런타임의 `npm audit` 경고 0건. 이는 감사 데이터베이스 기준이며 모든 취약점이 없다는 보장은 아니다.
- Windows의 짧은 임시 경로(`ADMINI~1`)에서 터미널 fixture의 문자열 비교 실패를 재현했다. 같은 폴더의 긴 경로에서는 통과하는 것을 확인한 뒤 fixture만 `realpath`로 정규화했다. 수정 후 짧은 경로·긴 경로 모두 통과했다.
- EXE 파일 버전 `2.1.1.0`, 실행 버전 `2.1.1` 확인. 격리 캐시에서 첫 추출과 재사용, 변경한 런타임 파일의 소스 해시 일치·import·Tesseract 실행을 확인했다.
- 다운로드 폴더의 로컬 패키지로 오프라인 npx 시작·브라우저 선택 전 취소가 정상 종료했다. npm registry 패키지를 실행한 검증은 아니다.
- 현재 설치기의 오프라인 합성 검사에서 정상 설치, 변조 EXE 거부, 변조 ZIP 거부, 실패 시 기존 EXE·프로필 보존을 확인했다. 공개 전이므로 2.1.1의 실제 GitHub 온라인 설치는 실행하지 않았다.
- PC 원본 소스·프로필·기존 결과·Tesseract를 보존했다. 승인된 KRAS 백업과 교체 중 생성한 백업, 이전 2.1.0 테스트 ZIP은 검증 후 정리했다. 다른 SCP·SMP 작업의 백업은 제외했다.
- GitHub의 공개 릴리스는 `v2.1.0`, `main`은 `dddccc258dbad853bfecc526973a7ce45706ada6`으로 유지했다. 새 커밋·푸시·태그·릴리스·npm 게시를 수행하지 않았다. 공개 npm 패키지의 현재 조회는 `E404`이며, 아래 2.0.3 게시 기록은 과거 관측이다.

```text
kras-quick.exe
7B706DFB3C2BB8B5E2CC888FDC1B771AB5B6199FAFE512BA1EEFCBF12BB4C4D6

kras-quick-runtime-v2.1.1.zip
C5522468466F4E46F7183BD1C3AB2C26E8AC2D75EDAEE74F38E4089765C913CC
```

## 2.1.0 (2026-10-27)

- Windows의 기능 검사 411건 통과. 전체 순차 실행에서 실패·건너뛰기 0건.
- 한글 경로의 실제 프로세스 식별 실패를 재현한 후, PowerShell UTF-8 출력 지정으로 기존 회귀검사가 통과했다.
- 타입검사 통과. 전체 린트는 오류 없이 기존 경고 14개가 남았다.
- 새 EXE의 파일 버전 `2.1.0.0`, 런타임 버전 `2.1.0` 확인.
- 격리 캐시에 runtime ZIP을 해시 검증 후 압축 해제하고, 런타임 import·Tesseract 실행·캐시 재사용 확인.
- 실제 EXE가 브라우저 선택 메뉴에 도달한 뒤 `0`으로 정상 종료했다. 브라우저 실행·KRAS 조회는 하지 않았다.
- 실행기 검사 1건과 오프라인 설치기 검사 통과. 잘못된 EXE·ZIP 체크섬 차단과 기존 프로필 보존을 검사했다.
- ZIP 2,514개 항목 중 변경한 소스는 2개이며, Tesseract 63개 항목은 이전 로컬 빌드와 동일하다.
- Windows에서 폰으로 옮긴 EXE·ZIP의 SHA-256이 빌드 결과와 일치한다.
- 병렬 검사 중 Node 검사 통신 역직렬화 오류가 한 번 발생했다. 해당 AUTO 검사 단독 실행 44건과 전체 순차 실행 411건은 통과했다.

이번 릴리스의 새 실제 보고서 조회·로그인·CAPTCHA·브라우저별 AUTO 및 스크린샷 QA는 실행하지 않았다. 이전 2.0.3 현장 성공을 2.1.0 현장 검증으로 간주하지 않는다. npm registry 게시도 수행하지 않는다. `tests/installed-runtime-smoke.mjs`, `tests/npm-installed-smoke.mjs`, `tests/npx-smoke.mjs`의 기존 2.0.3 registry 검증은 이번 2.1.0 검증에 포함하지 않는다.

```text
kras-quick.exe
50BF69D9367336AE26C4604AB388C7B7AD1469FC012FAE3BAD9116A2B1D9A70C

kras-quick-runtime-v2.1.0.zip
69892613B82B9F71011BB6D9E082BC0C03ACB57F82B311FF93B60ABAD0720762
```

## 2.0.3 (2026-10-07)

사용자 승인으로 전체 버전을 `2.0.3`으로 올렸다. GitHub Release와 Downloads는 반영했다.

- 기능 테스트 321건, 실행기 검사, 타입 검사 통과
- 새 runtime 압축 해제·전체 import·Tesseract 실행·캐시 재사용 통과
- 새 EXE 시작 화면 확인
- GitHub 온라인 설치와 SHA-256 확인 통과
- npm 게시 인증 완료, `PUBLISH_OK 202` 수신
- registry `latest=2.0.3` 확인
- registry의 npm 설치 후 EXE 다운로드·SHA-256 검증·2.0.3 시작 화면 확인
- 일반 `npx --yes @hayanjongseong/kras-quick@latest`로 EXE 다운로드·SHA-256 검증·2.0.3 시작 화면 확인
- npm 설치본과 npx를 각각 별도 임시 사용자 폴더에서 실행했다. 실제 테스트 Chrome의 CDP 연결, runtime 다운로드·압축 해제·해시 확인, 자식 런타임의 KRAS 로그인 안내, Tesseract 실행까지 모두 통과했다. 로그인이나 필지 조회는 제출하지 않았다.
- 실행 증거: `C:\Users\admin\AppData\Local\Temp\kras-npm-runtime-oobSwg\execution.json`, `C:\Users\admin\AppData\Local\Temp\kras-npx-runtime-HUKraq\execution.json`

반영 직후에는 registry 메타데이터의 버전 목록과 dist-tag가 잠시 어긋나 `ETARGET`가 발생했다. `--prefer-online`으로 재조회한 뒤 정상 설치를 확인했다. npx 검사는 현재 폴더의 옛 EXE가 명령을 가리지 않도록 빈 임시 폴더에서 실행했다.

```text
kras-quick.exe
50D50219D1B91D0F61553EA5A204425836BBB51EB09F283BCC272132247E6A64

kras-quick-runtime-v2.0.3.zip
C85CBBAFE13BA90F1D3327C31524507DA8232619FE09D800C666F5BFAB1E9D62
```

아래는 이전 `2.0.2` 수정본의 기록이다.

## 2.0.2 수정본 검증

검증일: 2026-10-07. 버전과 Release 태그는 그대로 유지했다.

## 확인한 항목

- `npm run typecheck`: 통과
- `npm run test:kras`: 144건 통과, 제외한 테스트 없음
- `npm run test:property`: 177건 통과
- `npm test`: 실행기 BOM·문법·다운로드 파일명 검사 통과
- `tests/test-installer.ps1`: 체크섬 오류 차단, 버전명 runtime 설치, 프로필 보존 검사 통과
- GitHub 소스에서 두 빌드 스크립트 실행: 통과
- `tests/runtime-smoke.mjs`: runtime SHA-256, 압축 해제, 전체 import, Tesseract 실행, 캐시 재사용, EXE 시작 화면 확인
- GitHub의 `install.ps1`을 실제 다운로드해 격리 폴더에 설치: 통과. 프로필·결과 폴더 보존 확인
- GitHub main.tar.gz를 npm으로 격리 설치: `@hayanjongseong/kras-quick@2.0.2` 확인
- `tests/npm-installed-smoke.mjs`: 설치된 `kras-quick.cmd`로 EXE 다운로드·SHA-256 검증·시작 화면까지 확인
- Downloads 수정본 복사 후 해시 일치 확인. 교체 전 파일은 백업했다.

## 배포 파일 해시

```text
kras-quick.exe
2C21D190ADCAE80D8C37DEF9EA80715B717B8D9E7DE52976377D6A9EE177AAC8

kras-quick-runtime-v2.0.2.zip
EF3F61C449F1EA3A4FBE240A07139F519669B44997C8A00F20F42E8E84592DB5
```

## 남은 제한

npm registry에 이미 게시한 `2.0.2` tarball은 덮어쓸 수 없다. 해당 파일의 BOM·URL 오류는 남아 있다. 삭제·버전 변경은 하지 않았으며, README에 안내한 GitHub tar.gz 설치 경로는 검증했다.

실제 로그인 계정으로 KRAS 조회, CAPTCHA 처리, OZ PDF 저장을 끝까지 진행하는 현장 검증은 하지 않았다. 자동 테스트와 시작 화면 확인을 현장 검증으로 대체해 보고하지 않는다.

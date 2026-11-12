# 배포 검증 기록

## 2.1.2 정식 게시 (2026-11-12)

- AUTO 보안문자 재시도 번호의 OCR 옵션 전달 오류를 수정했다. 실제 OCR 함수와 연결한 회귀검사로 수정 전 동일 설정·후보 반복을 재현했고 수정 후 1·2·3차 설정 구분과 중복 후보 제출 방지·수동 전환을 확인했다. 실제 사이트 보안문자나 로그인을 제출하지 않았다.
- 실제 Edge의 격리된 로컬 응답 페이지에서 `Execution context was destroyed` 오류를 재현했다. 문서 로딩을 기다린 뒤 같은 조회가 2회차에 복구되는 것을 확인했다. 이 재시도는 주소 조회에만 적용하며 보안문자 제출·저장 작업 전체를 반복하지 않는다.
- CLI 일괄처리 회귀검사에서 실패 지번·원본 PNU·사유가 종료 요약에 남고, 나머지 주소가 계속 처리되며 동일 주소의 재처리가 성공하면 실패 목록에서 제거되는 것을 확인했다. 실패 목록은 현재 실행의 메모리·콘솔 요약이며 파일 저장이나 강제 종료 후 복구 기능은 아니다.
- Windows PowerShell에서 `Expand-Archive`의 `ExtractToDirectory` 인수 호환 오류 3건을 재현했다. .NET의 2인자 호출로 교체한 뒤 관련 16/16, 전체 437/437, 타입검사·전체 린트가 통과했다. 시스템 PowerShell 모듈은 변경하지 않았다.
- 기존 약 104MB runtime ZIP의 SHA-256 확인·한글/공백/따옴표 경로 추출·네이티브 Node/Tesseract 실행·캐시 재사용을 검증했다. 아래 이전 버전의 현장 성공 기록과 새 2.1.2 배포본 검증은 구분한다.
- 새 EXE의 파일 버전 `2.1.2.0`과 외부 ZIP의 실행 버전 `2.1.2`가 일치한다. 한글·공백·따옴표를 포함한 설치 경로에서 새 EXE의 첫 실행·추출·브라우저 선택 전 `0` 취소와 두 번째 캐시 재사용을 확인했다.
- 새 ZIP에 포함된 수정 소스 5개의 SHA-256이 PC 소스와 일치하며 새 navigation·실패 집계 모듈도 포함됐다. 추출한 실제 runtime의 import 검사와 `npm audit --omit=dev` 경고 0건을 확인했다.
- 전체 개발용 포터블의 OpenCode 플러그인 → Effect 의존성에는 중간 등급 경고 2건이 남아 있다(`GHSA-chp3-j5qr-2vfr`). 이번 EXE용 축소 ZIP에는 해당 플러그인·Effect가 없으며, 관련 없는 플러그인 다운그레이드는 하지 않았다. 축소 runtime의 감사 결과와 전체 포터블 결과를 혼동하지 않는다.
- 새 npm 준비본은 `kras-quick@2.1.2`이며 bin·LICENSE·package.json·README만 포함한다. 실행기 검사 1/1과 게시 도우미 자체검사를 통과했다. 이 빌드 시점 검사와 아래 실제 공개 게시·설치 결과를 구분한다.
- GitHub Release `v2.1.2`를 새 태그·릴리스로 공개했다. EXE·runtime ZIP·체크섬 2개의 익명 다운로드와 SHA-256 대조가 통과했다. 게시 태그는 `1ae879371b36e409cf0fc7863fd3cfd1b1488189`에 두고 기존 릴리스는 교체하지 않았다.
- 휴대폰 패스키로 npm 로그인·게시를 승인한 뒤 `kras-quick@2.1.2`와 `latest=2.1.2`의 공개 반영을 확인했다. 공개 tarball SHA-512가 준비본과 일치한다. 접수 `202`와 공개 완료를 구분했고 전파 대기 중 재게시하지 않았다.
- Windows의 별도 사용자 폴더·빈 작업 폴더·새 캐시에서 공개 `npm install -g kras-quick` 후 실행과 `npx kras-quick`을 각각 검증했다. 둘 다 2.1.2로 시작해 브라우저 선택 전 `0` 취소로 정상 종료했고 다운로드한 EXE SHA-256이 일치했다.
- 공개 `main/install.ps1`을 메모리에서 받아 격리 경로에 `-NoLaunch`로 설치했다. 실제 내려받은 EXE·runtime ZIP SHA-256이 일치한다. 실제 KRAS 로그인·필지 조회·전체 AUTO의 현장 검증은 새로 실행하지 않았다.

```text
kras-quick.exe
F6DB476BDF664A5F4C51BC24998D3DBCC3514A970F7C3E322410C64F5ABC606A

kras-quick-runtime-v2.1.2.zip
C6D2CC48BF2C26A80DEE7E0453A1379A2B8100CBC69D439BED4445F13B0D703A

kras-quick@2.1.2
sha512-NOiv6aUxgXbj1FuTsVLNXUhReZs+F/4fwaJ1xRmelczpfXVkFIQpy6rIBFHwIXrBe8AaLUgb0tPlrXfFH/PifA==
```

## 2.1.1 정식 게시 (2026-10-27)

- GitHub Release `v2.1.1`의 EXE·runtime ZIP·체크섬 2개를 공개했다. 네 자산의 익명 다운로드와 SHA-256 대조가 통과했다. 기존 릴리스·태그는 보존했다.
- 공개 `main/install.ps1`을 받아 격리 폴더에 `-NoLaunch`로 설치하고 EXE·runtime ZIP의 SHA-256을 확인했다.
- 확정 npm 이름은 `kras-quick`이다. `kras-quick@2.1.1`의 공개 조회, `latest=2.1.1`, 공개 tarball과 준비본의 SHA-512 일치를 확인했다.
- Windows의 빈 작업 폴더·별도 사용자 폴더·새 캐시에서 `npm install -g kras-quick` 후 실행과 `npx kras-quick`을 각각 검증했다. 두 경로 모두 2.1.1 시작·브라우저 선택 메뉴·`0` 취소 정상 종료·EXE SHA-256 검증을 통과했다. 실제 KRAS 로그인·조회는 제출하지 않았다.
- `package.json`·lockfile·bin 이름과 `npx kras-quick` 계약을 실행기 검사에 추가했다. 잘못된 scoped 이름에서 실패하고 수정한 이름에서 통과하는 것을 확인했다.
- 앞서 실수로 게시한 scoped 패키지의 검증은 위 unscoped 패키지 검증의 대체 근거로 사용하지 않았다. 사용자 승인과 모바일 패스키 승인 후 scoped 패키지 전체를 삭제했다. 공개 registry의 버전 목록이 비었고 삭제 이력만 남으며 scoped 2.1.1 조회는 404인 것을 확인했다. 정상 `kras-quick@2.1.1`은 유지했다.
- 게시 도중 `E409`를 한 차례 받았다. 상세 응답이 남지 않아 원인을 확정하지 않았고, 이를 버전 중복이나 삭제 이력 때문이라고 단정하지 않았다. 이후 동일 버전이 접수됐고 공개 반영을 확인한 다음 실행을 검증했다.

```text
kras-quick@2.1.1
sha512-YaJRDJhyy9DhDa2VBFnLhEa2vEeY6WhzXh5fI+MKlLR3LkMnUQFVx7Qg+Ms9tT48BFcU2S5azdAuDWQfGUBVqg==
```

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

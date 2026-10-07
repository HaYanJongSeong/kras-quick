# 2.0.2 수정본 검증

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

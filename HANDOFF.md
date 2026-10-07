# KRAS Quick Handoff

작성일: 2026-10-07

## 현재 상태

- 프로젝트: `C:\Project\시유재산\kras-quick`
- AUTO에서 지번이 없으면 안내창의 `OK`를 누르고, 해당 주소를 기록한 뒤 다음 주소를 처리한다.
- AUTO는 `일시정지`와 `재개`를 지원한다.
- 로그인이 만료되면 재시도를 멈추고 재로그인을 요청한다.
- 도호 불일치·출력축척 안내창은 자동으로 확인한다.
- 처리 중에는 건수를 표시하고, 종료할 때는 목록을 출력한다.

## 검증

- KRAS 테스트 144건, property 테스트 177건 통과
- `build-portable.ps1` 통과
- `build-single-exe.ps1` 통과
- Archify showcase validation, deliver, visual-check 통과
- 전체 typecheck 통과. 누락된 `tools/kras.ts` 복원, 터미널 인자 누락 수정
- 설치 테스트, runtime 모듈 로딩, Tesseract 실행, EXE 시작 화면 확인
- 실제 KRAS 로그인·조회·OZ PDF 저장은 미검증

## npm 배포 주의

npm registry의 `2.0.2`에는 BOM·다운로드 파일명 오류가 남아 있다. 같은 버전을 덮어쓸 수 없으며, 사용자 요청에 따라 버전은 올리지 않았다. README의 GitHub tar.gz 설치 명령으로 수정본을 설치한다.

## 배포 파일

- `C:\Users\admin\Downloads\kras-quick.exe` (v2.0.2)
- `C:\Users\admin\Downloads\kras-quick-runtime-v2.0.2.zip`
- GitHub Release: `https://github.com/HaYanJongSeong/kras-quick/releases/tag/v2.0.2`

실행 중인 EXE는 종료한 뒤 교체한다.

`build-single-exe.ps1`는 Tesseract 설치 경로를 사용한다.

## 주의

- Chrome/Brave 프로세스를 자동 종료하지 않는다.
- 잠긴 Downloads 파일을 강제 종료하지 않는다.

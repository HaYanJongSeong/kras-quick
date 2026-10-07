# KRAS Quick Handoff

작성일: 2026-10-01

## 현재 상태

- 프로젝트: `C:\Project\시유재산\kras-quick`
- AUTO에서 지번이 없으면 안내창의 `OK`를 누르고, 해당 주소를 기록한 뒤 다음 주소를 처리한다.
- AUTO는 `일시정지`와 `재개`를 지원한다.
- 로그인이 만료되면 재시도를 멈추고 재로그인을 요청한다.
- 도호 불일치·출력축척 안내창은 자동으로 확인한다.
- 처리 중에는 건수를 표시하고, 종료할 때는 목록을 출력한다.

## 검증

- `kras-workflow-watcher` 테스트: `20/20` 통과
- `build-portable.ps1` 통과
- `build-single-exe.ps1` 통과
- Archify showcase validation, deliver, visual-check 통과
- 전체 typecheck는 기존 `tools/kras.ts` 누락으로 실패

## 배포 파일

- `C:\Users\admin\Downloads\kras-quick.exe` (v2.0.2)
- `C:\Users\admin\Downloads\kras-quick-runtime-v2.0.2.zip`
- GitHub Release: `https://github.com/HaYanJongSeong/kras-quick/releases/tag/v2.0.2`

실행 중인 EXE는 종료한 뒤 교체한다.

`build-single-exe.ps1`는 Tesseract 설치 경로를 사용한다.

## 주의

- Chrome/Brave 프로세스를 자동 종료하지 않는다.
- 잠긴 Downloads 파일을 강제 종료하지 않는다.

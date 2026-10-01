# KRAS Quick Handoff

작성일: 2026-10-01

## 현재 상태

- 프로젝트: `C:\Project\시유재산\kras-quick`
- AUTO 지번 없음 처리 완료
- 지번 없음 창의 `OK` 자동 클릭
- 지번 없음 주소 기록 후 다음 주소 계속 처리
- AUTO `일시정지`/`재개` 지원
- 로그인 만료 시 재시도 중단 및 재로그인 요청
- 도호 불일치·출력축척 안내창 자동 확인
- 진행 중 건수 표시 및 종료 목록 출력

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

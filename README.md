<p align="center"><img src="https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/assets/kras-quick.png" width="128" height="128" alt="KRAS Quick"></p>

# KRAS Quick

Windows용 부동산종합증명서 조회·저장 도구입니다. 버전 3.0.0부터 Rust 런타임을 사용합니다.

실사이트 전체 기능 검증은 완료되지 않았습니다. 접속 차단·보안문자 반복 실패가 발생하면 실행을 중단하고 서비스 운영기관의 안내를 따르세요.

```text
npx kras-quick
```

Windows x64와 Node.js 18 이상이 필요합니다. Node.js 없이 사용하려면 [3.0.1 릴리스](https://github.com/HaYanJongSeong/kras-quick/releases/tag/v3.0.1)의 EXE를 사용하세요. 최초 실행 시 OCR 모델 런타임을 GitHub에서 준비합니다.

변경사항과 검증 범위는 아래에 정리했습니다. 저장소의 이전 설치 스크립트는 3.0.x용으로 검증하지 않았습니다.

## 산출물

직접 실행하면 EXE 폴더에 저장합니다. npx는 `%USERPROFILE%\Downloads\kras-quick.exe`를 설치·실행하고, `%USERPROFILE%\Downloads\KRAS\지번\`에 저장합니다. 기존 업무자료는 이동·삭제하지 않습니다. Windows에서 다운로드 폴더를 다른 위치로 변경한 경우에도 현재 설치기는 위 고정 경로를 사용합니다.

```text
KRAS/지번/
├─ 지번-건물명.pdf
└─ 기타/
   ├─ 지번-건물명.xml
   ├─ 지번-건물명.json
   └─ 작업로그.jsonl
```

토지는 건물명 없이, 층호 문서는 층호까지 파일명에 표기합니다. 정상 산출물이 확인되면 중복 출력은 건너뜁니다.

XML은 OZ 원문입니다. 같은 이름의 JSON은 `document`에 전체 태그·속성·계층·반복 항목·빈 값·문자열을 보존하고, `summary`에 주요 항목을 따로 둡니다. 단계·오류·무결성 기록은 `작업로그.jsonl` 하나에 누적합니다. PDF에 사용한 PNG는 별도로 저장하지 않습니다.

## 3.0.1까지의 변경사항

### 3.0.0 · Rust 전환

실행 본체를 Rust로 옮기고 Python OCR 워커를 제거했습니다. `npx kras-quick`은 그대로 쓰며, Node.js는 EXE를 실행하는 역할만 맡습니다.

저장 파일도 정리했습니다. PDF와 원문 XML에 전체 정보 JSON을 더했고, 단계별 기록은 `작업로그.jsonl` 하나로 합쳤습니다. JSON에는 XML의 태그·속성·계층·반복 항목·빈 값과 문자열을 보존합니다. PDF를 만들 때 쓴 PNG는 따로 저장하지 않습니다.

### 3.0.1 · 아이콘과 오류 수정

EXE와 GitHub·npm 문서는 남색 바탕에 흰 종이비행기와 파란 `+`가 들어간 아이콘을 사용합니다. 아이콘은 3.0.0부터 적용돼 있습니다.

- `npx kras-quick` 실행 시 EXE를 다운로드 폴더에 설치하고, 같은 폴더의 `KRAS`에 결과를 저장하도록 고쳤습니다. 기존 자료는 옮기거나 삭제하지 않습니다.
- 처리 실패나 보안문자 입력 취소·EOF·접속 차단이 발생하면 배치를 멈춥니다. 미처리 주소는 현재 실행 중인 프로그램의 대기 목록에 남기며, 모드를 바꿔도 지우지 않습니다.
- 동시에 저장하더라도 기존 PDF·XML·JSON을 덮어쓰지 않도록 수정했습니다. 손상된 JSON을 이전 형식의 검증 결과로 건너뛰던 문제도 고쳤습니다.
- 입력이 끝났을 때 전체 처리가 자동으로 시작되는 문제를 막았습니다.

### 확인한 범위

3.0.1은 Rust 테스트 419개, 설치기 테스트 5개와 접속 차단 안내 3종 검사를 통과했습니다. Windows EXE 빌드와 로컬 npm 설치·버전 실행도 확인했습니다. 별도 조건이 필요한 테스트 8개는 제외했으며, 실사이트 전체 기능과 속도는 아직 검증하지 않았습니다.

접속 차단이 풀리기 전에는 조회하지 마세요. 저장 도중 오류가 나면 일부 새 파일이 남아 재실행이 중단될 수 있습니다. 대기 목록은 프로그램을 종료한 뒤까지 보관하지 않습니다.

## 아이콘

앱·문서·패키지는 남색 바탕의 흰 종이비행기와 파란 `+` 아이콘으로 통일합니다.
일사편리 파비콘을 참고한 별도 도구용 아이콘이며, 정부 공식 앱을 뜻하지 않습니다.

## 개발

Rust 소스는 `rust/`에 있습니다. Windows SDK의 리소스 컴파일러가 필요합니다.

```powershell
cargo test --manifest-path rust/Cargo.toml --locked
cargo build --manifest-path rust/Cargo.toml --release --locked
```

실사이트 테스트는 기본 실행에서 제외합니다. [MIT 라이선스](LICENSE)를 따릅니다.

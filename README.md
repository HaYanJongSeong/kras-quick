<p align="center"><img src="https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/assets/kras-quick.png" width="128" height="128" alt="KRAS Quick"></p>

# KRAS Quick

Windows용 부동산종합증명서 조회·저장 도구입니다. 버전 3.0.0부터 Rust 런타임을 사용합니다.

실사이트 전체 기능 검증과 일부 복구 작업은 완료되지 않았습니다. 접속 차단·보안문자 반복 실패가 발생하면 실행을 중단하고 서비스 운영기관의 안내를 따르세요.

```text
npx kras-quick
```

Windows x64와 Node.js 18 이상이 필요합니다. Node.js 없이 사용하려면 [3.0.0 릴리스](https://github.com/HaYanJongSeong/kras-quick/releases/tag/v3.0.0)의 EXE를 사용하세요. 최초 실행 시 OCR 모델 런타임을 GitHub에서 준비합니다.

변경 사항과 검증 제한은 [릴리스 노트](RELEASE_NOTES_v3.0.0.md)에 있습니다. 이전 버전 설치 스크립트는 3.0.0용으로 검증되지 않았습니다.

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

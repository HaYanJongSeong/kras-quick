# KRAS-QUICK
## 📦 설치

Node.js 18 이상에서 npm으로 설치하고 실행합니다.

```powershell
npm install -g @hayanjongseong/kras-quick
kras-quick
```

첫 실행 때 GitHub에서 launcher EXE를 내려받고 검증합니다. EXE가 runtime ZIP을 다시 내려받아 `%LOCALAPPDATA%\kras-quick`에 캐시합니다. `npm install` 중에는 큰 파일을 받지 않습니다.

## GitHub 직접 설치

PowerShell에서 아래 명령 하나로 설치하고 실행할 수도 있습니다. git·gh 불필요.

```powershell
irm https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/install.ps1 | iex
```

> GitHub Releases에서 launcher `kras_quick_v1.0.6.exe`를 받아 SHA-256 검증 후 `Downloads\kras-quick.exe`로 원자적으로 교체하고 실행합니다. 설치 스크립트 원문: [`install.ps1`](install.ps1)

## ⚠️ 요구사항

- Windows 10 이상
- Chrome 설치 (브라우저 프로필은 `%USERPROFILE%\Downloads\.kras-chrome-profile`에 유지)
- 인터넷 연결

## 🔒 비공식 도구 고지

이 도구는 **KRAS·일사편리·OZ Viewer·Softgraphy·Google·Microsoft**와 **제휴·승인·보증 관계가 없습니다**. 사용으로 인한 모든 책임은 사용자 본인에게 있습니다.

## 📄 라이선스

[MIT](LICENSE) © 2026 Seong

---

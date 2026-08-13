# KRAS-QUICK
## 📦 설치 (한 줄)

git·gh 없이 PowerShell 하나로 설치하고 실행합니다.

```powershell
$u='https://github.com/HaYanJongSeong/kras-quick/releases/download/v0.2.1/kras_quick_v0.2.1.exe';$d="$env:USERPROFILE\Downloads";Invoke-WebRequest $u -OutFile "$d\kras_quick_v0.2.1.exe";Invoke-WebRequest "$u.sha256" -OutFile "$d\kras_quick_v0.2.1.exe.sha256";$h=((Get-Content "$d\kras_quick_v0.2.1.exe.sha256" -Raw) -split '\s+')[0];$a=(Get-FileHash "$d\kras_quick_v0.2.1.exe" -Algorithm SHA256).Hash;if($h -ne $a){throw 'SHA-256 mismatch'};$t="$d\kras-quick.exe.$([guid]::NewGuid().ToString('N')).tmp";Copy-Item "$d\kras_quick_v0.2.1.exe" $t;Move-Item $t "$d\kras-quick.exe" -Force;Start-Process "$d\kras-quick.exe"
```

> `kras_quick_v0.2.1.exe.sha256` 해시와 일치해야 설치되며, 기존 `kras-quick.exe`만 원자적으로 교체됩니다.

## ⚠️ 요구사항

- Windows 10 이상
- Chrome 설치 (브라우저 프로필은 `%USERPROFILE%\Downloads\.kras-chrome-profile`에 유지)
- 인터넷 연결

## 🔒 비공식 도구 고지

이 도구는 **KRAS·일사편리·OZ Viewer·Softgraphy·Google·Microsoft**와 **제휴·승인·보증 관계가 없습니다**. 사용으로 인한 모든 책임은 사용자 본인에게 있습니다.

## 📄 라이선스

[MIT](LICENSE) © 2026 Seong

---

; ═══════════════════════════════════════════════════════════
; BypaxDPI NSIS Installer Hooks
; ═══════════════════════════════════════════════════════════

; ─── KURULUM ÖNCESİ ───
!macro NSIS_HOOK_PREINSTALL
    ; Eski sürüm çalışıyorsa kapat
    nsExec::ExecToStack 'taskkill /F /IM BypaxDPI.exe'
    Pop $0
    nsExec::ExecToStack 'taskkill /F /IM bypax-proxy.exe'
    Pop $0
    Sleep 500

    ; Proxy temizle (upgrade sırasında internet kopmasın)
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyEnable" 0
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyServer"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyOverride"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "AutoConfigURL"

    ; WinHTTP sıfırla
    nsExec::ExecToStack 'netsh winhttp reset proxy'
    Pop $0

    ; Sentinel temizle
    Delete "$TEMP\bypaxdpi_proxy_active.lock"
    Delete "$TEMP\bypaxdpi_sidecar.pid"
!macroend

; ─── KALDIRMA ÖNCESİ ───
!macro NSIS_HOOK_PREUNINSTALL
    ; 0. Uygulamayı kapat
    nsExec::ExecToStack 'taskkill /F /IM BypaxDPI.exe'
    Pop $0
    Sleep 1000

    ; 1. Proxy ayarlarını sıfırla (WinINet)
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyEnable" 0
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyServer"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyOverride"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "AutoConfigURL"

    ; 1.5 WinHTTP Proxy ayarlarını sıfırla (Kritik: Arka plan servisleri bozulmasın)
    nsExec::ExecToStack 'netsh winhttp reset proxy'
    Pop $0

    ; 2. Sentinel ve PID dosyalarını temizle
    Delete "$TEMP\bypaxdpi_proxy_active.lock"
    Delete "$TEMP\bypaxdpi_sidecar.pid"

    ; 3. Zombi sidecar öldür
    nsExec::ExecToStack 'taskkill /F /IM bypax-proxy.exe'
    Pop $0

    ; 4. Firewall kurallarını temizle
    nsExec::ExecToStack 'netsh advfirewall firewall delete rule name=BypaxDPI_Proxy'
    Pop $0
    nsExec::ExecToStack 'netsh advfirewall firewall delete rule name=BypaxDPI_PAC'
    Pop $0

    ; 5. Autostart registry kaydını temizle
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "BypaxDPI"

    ; 6. DNS önbelleğini temizle
    nsExec::ExecToStack 'ipconfig /flushdns'
    Pop $0
!macroend

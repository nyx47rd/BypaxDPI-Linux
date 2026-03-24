; P0-FIX: Uninstall sırasında internet bağlantısının kopuk kalmasını engelle
!macro NSIS_HOOK_UNINSTALL
    ; 1. Proxy ayarlarını sıfırla (uygulama o an çalışmıyorsa bile garantiye al)
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyEnable" 0
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyServer"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyOverride"
    
    ; 2. Sentinel dosyasını temizle (dirty-state kilidi)
    Delete "$TEMP\bypaxdpi_proxy_active.lock"
    
    ; 3. Olası çalışan zombi sidecar'ı sessizce öldür
    nsExec::ExecToLog 'taskkill /F /IM bypax-proxy.exe'
!macroend

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use local_ip_address::list_afinet_netifas;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use tauri::Emitter;
use tauri::Manager;

/// Sanal ağ adaptörlerini filtreleyen akıllı LAN IP bulucu.
/// VirtualBox, VMware, Hamachi, VPN gibi sanal adaptörleri atlar.
fn get_safe_lan_ip() -> String {
    // Filtrelenecek sanal adaptör anahtar kelimeleri (küçük harf)
    const VIRTUAL_KEYWORDS: &[&str] = &[
        "virtual",
        "vmware",
        "vmnet",
        "vbox",
        "virtualbox",
        "pseudo",
        "hamachi",
        "vpn",
        "vethernet",
        "loopback",
        "docker",
        "wsl",
        "hyper-v",
        "bluetooth",
        "teredo",
        "isatap",
        "6to4",
    ];

    if let Ok(netifs) = list_afinet_netifas() {
        // Önce IPv4 adresleri arasında gerçek adaptörü bul
        for (name, ip) in &netifs {
            // Sadece IPv4
            if let IpAddr::V4(v4) = ip {
                // Loopback ve link-local adresleri atla
                if v4.is_loopback() || v4.is_link_local() {
                    continue;
                }
                // Sanal adaptör mü kontrol et
                let name_lower = name.to_lowercase();
                let is_virtual = VIRTUAL_KEYWORDS.iter().any(|kw| name_lower.contains(kw));
                if !is_virtual {
                    return v4.to_string();
                }
            }
        }
        // Hiç gerçek adaptör bulunamazsa, sanal olmayanları da dene (IPv4)
        for (_, ip) in &netifs {
            if let IpAddr::V4(v4) = ip {
                if !v4.is_loopback() && !v4.is_link_local() {
                    return v4.to_string();
                }
            }
        }
    }
    // Fallback
    "127.0.0.1".to_string()
}

/// PAC sunucusu durumu: thread handle + shutdown flag + dinamik body
pub struct PacServerState {
    pub join_handle: Mutex<Option<thread::JoinHandle<()>>>,
    pub shutdown: Arc<AtomicBool>,
    pub pac_body: Arc<Mutex<String>>,
    pub pac_port: Mutex<u16>,
    pub pac_url: Mutex<String>,
}

impl Default for PacServerState {
    fn default() -> Self {
        Self {
            join_handle: Mutex::new(None),
            shutdown: Arc::new(AtomicBool::new(false)),
            pac_body: Arc::new(Mutex::new(make_pac_direct_body())),
            pac_port: Mutex::new(0),
            pac_url: Mutex::new(String::new()),
        }
    }
}

const PAC_PORT_START: u16 = 8787;
const PAC_PORT_END: u16 = 8887;
const SUPPORT_URL: &str = "https://www.patreon.com/join/ConsolAktif";

/// Bağlantı kesildiğinde kullanılan fallback PAC: tüm trafiği DIRECT yönlendirir
/// Bu sayede cihazlar internet erişimini kaybetmez
fn make_pac_direct_body() -> String {
    r#"function FindProxyForURL(url, host) {
    return "DIRECT";
}
"#
    .to_string()
}

/// Production PAC: yerel ağ DIRECT, diğerleri PROXY ip:port; DIRECT (fail-safe)
/// dnsResolve çağrıları try-catch ile korunuyor — DNS timeout olursa PAC script çökmez
fn make_pac_body(lan_ip: &str, proxy_port: u16) -> String {
    let proxy = format!("{}:{}", lan_ip, proxy_port);
    format!(
        r#"function FindProxyForURL(url, host) {{
    if (isPlainHostName(host) || host === "localhost" || host.indexOf("127.") === 0 ||
        shExpMatch(host, "*.local"))
        return "DIRECT";
    try {{
        var resolved = dnsResolve(host);
        if (resolved &&
            (isInNet(resolved, "192.168.0.0", "255.255.0.0") ||
             isInNet(resolved, "10.0.0.0", "255.0.0.0") ||
             isInNet(resolved, "172.16.0.0", "255.240.0.0") ||
             isInNet(resolved, "127.0.0.0", "255.0.0.0")))
            return "DIRECT";
    }} catch(e) {{}}
    return "PROXY {}; DIRECT";
}}
"#,
        proxy
    )
}

fn make_setup_html(pac_url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="tr">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>BypaxDPI – Proxy Kurulum</title>
<style>
* {{ box-sizing: border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 0; background: linear-gradient(180deg, #0c0c0e 0%, #0f0f0f 100%); color: #e4e4e7; min-height: 100vh; }}
.brand-header {{ text-align: center; padding: 24px 16px 20px; background: linear-gradient(135deg, rgba(59,130,246,0.08) 0%, transparent 50%); border-bottom: 1px solid rgba(255,255,255,0.06); }}
.brand-name {{ font-size: 1.5rem; font-weight: 800; margin: 0; letter-spacing: 0.05em; color: #fafafa; }}
.brand-tagline {{ font-size: 0.85rem; color: #94a3b8; margin: 4px 0 0; }}
.main {{ padding: 16px; max-width: 400px; margin: 0 auto; }}
p {{ font-size: 0.9rem; color: #a1a1aa; line-height: 1.5; margin: 0 0 12px; }}
.url-input {{ width: 100%; background: #27272a; border: 1px solid #3f3f46; border-radius: 10px; padding: 12px; font-size: 0.85rem; color: #e4e4e7; margin: 8px 0; -webkit-user-select: all; user-select: all; }}
.btn {{ display: inline-block; background: #3b82f6; color: #fff; border: none; padding: 12px 24px; border-radius: 10px; font-size: 1rem; font-weight: 600; cursor: pointer; margin: 8px 4px 8px 0; width: 100%; max-width: 280px; }}
.btn:active {{ opacity: 0.9; }}
.btn-android {{ background: #22c55e; font-size: 1.05rem; padding: 14px; }}
.link {{ color: #60a5fa; text-decoration: none; display: inline-block; margin: 8px 0; }}
.link:active {{ text-decoration: underline; }}
h2 {{ font-size: 0.95rem; color: #d4d4d8; margin: 20px 0 8px; }}
ul {{ margin: 0; padding-left: 20px; color: #a1a1aa; font-size: 0.88rem; line-height: 1.6; }}
.copied {{ background: #22c55e !important; }}
.step {{ margin: 10px 0; padding: 12px; background: rgba(34,197,94,0.08); border-radius: 12px; border-left: 4px solid #22c55e; }}
</style>
</head>
<body>
<header class="brand-header">
  <h1 class="brand-name">BypaxDPI</h1>
  <p class="brand-tagline">Proxy Kurulum</p>
</header>
<main class="main">
<p><strong>Android:</strong> Önce «Kopyala»ya bas, sonra aşağıdaki adımları izle. <strong>iPhone:</strong> Aşağı kaydır.</p>
<input type="text" class="url-input" id="pacurl" value="{}" readonly onclick="this.select();">
<button class="btn btn-android" id="copybtn">Kopyala (önce buna bas)</button>
<p id="copyhint" style="font-size:0.8rem;color:#71717a;margin-top:0;">Kopyalamadıysa yukarıdaki kutuya uzun bas → Kopyala</p>
<script>
(function(){{ var url=document.getElementById('pacurl').value; var btn=document.getElementById('copybtn'); var hint=document.getElementById('copyhint');
function tryCopy(){{ if(navigator.clipboard && navigator.clipboard.writeText){{ navigator.clipboard.writeText(url).then(function(){{ btn.textContent='Kopyalandı!'; btn.classList.add('copied'); hint.textContent='Şimdi Ayarlar\'a gidip proxy alanına yapıştırın.'; setTimeout(function(){{ btn.textContent='Tekrar kopyala'; btn.classList.remove('copied'); }}, 3000); }}).catch(function(){{ selectAndHint(); }}); }} else selectAndHint(); }}
function selectAndHint(){{ var inp=document.getElementById('pacurl'); inp.select(); inp.setSelectionRange(0,99999); hint.textContent='Kutuyu seçildi. Uzun bas → Kopyala seçin.'; }}
btn.onclick=tryCopy; document.getElementById('pacurl').onclick=function(){{ this.select(); }}; }})();
</script>
<h2>iPhone / iPad</h2>
<ul>
<li>Ayarlar → Wi-Fi → Bağlı ağın yanındaki (i) → Proxy Yapılandırması → Otomatik → URL’yi yapıştırın.</li>
</ul>
<div class="step"><strong>Android adımlar:</strong>
<ol style="margin:8px 0 0 16px; padding:0; color:#a1a1aa; font-size:0.9rem; line-height:1.7;">
<li>Yukarıdan adresi kopyala</li>
<li><a href="intent://android.settings.WIRELESS_SETTINGS#Intent;scheme=android-app;package=com.android.settings;end" class="link">Ayarlar / Ağ veya Wi-Fi'yi aç</a></li>
<li>Bağlı olduğun Wi-Fi ağına dokun (veya ağ adının yanındaki dişli/ok)</li>
<li>Gelişmiş veya Proxy bölümüne gir</li>
<li>Proxy: Otomatik yapılandırma veya PAC seç</li>
<li>PAC adresi / URL alanına yapıştır</li>
</ol></div>
</main>
</body>
</html>"#,
        html_escape(pac_url)
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn handle_pac_request(mut stream: TcpStream, pac_body: &Arc<Mutex<String>>, pac_url: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let mut buf = [0u8; 512];
    let first_line = match stream.read(&mut buf) {
        Ok(n) if n > 0 => std::str::from_utf8(&buf[..n])
            .ok()
            .and_then(|s| s.lines().next())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    };
    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/");
    let is_get = first_line.to_uppercase().starts_with("GET ");

    if is_get && path == "/logo" {
        let img = include_bytes!("../icons/128x128.png");
        let hdr = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            img.len()
        );
        let _ = stream.write_all(hdr.as_bytes());
        let _ = stream.write_all(img);
        let _ = stream.flush();
        return;
    }

    // PAC body'yi lock'tan oku — bağlantı durumuna göre dinamik olarak değişir
    let current_pac_body = pac_body
        .lock()
        .map(|b| b.clone())
        .unwrap_or_else(|_| make_pac_direct_body());

    let (status, content_type, body) = if !is_get {
        ("404 Not Found", "text/plain", String::new())
    } else if path == "/proxy.pac" {
        (
            "200 OK",
            "application/x-ns-proxy-autoconfig",
            current_pac_body,
        )
    } else if path == "/" || path.is_empty() {
        (
            "200 OK",
            "text/html; charset=utf-8",
            make_setup_html(pac_url),
        )
    } else {
        ("404 Not Found", "text/plain", String::new())
    };

    // Cache-Control: no-cache ekle — cihazlar her seferinde güncel PAC'ı alsın
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nConnection: close\r\nCache-Control: no-cache, no-store, must-revalidate\r\nPragma: no-cache\r\nContent-Length: {}\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[derive(serde::Serialize)]
struct PacResponse {
    pac_port: u16,
}

#[tauri::command]
fn start_pac_server(
    proxy_port: u16,
    state: tauri::State<'_, PacServerState>,
) -> Result<PacResponse, String> {
    let lan_ip = get_safe_lan_ip();

    // PAC body'yi güncelle — proxy moduna geç
    let new_pac_body = make_pac_body(&lan_ip, proxy_port);
    if let Ok(mut body) = state.pac_body.lock() {
        *body = new_pac_body;
    }

    // Sunucu zaten çalışıyorsa, sadece body güncellendi — port bilgisini döndür
    let guard = state.join_handle.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        let current_port = *state.pac_port.lock().map_err(|e| e.to_string())?;
        // PAC URL'yi de güncelle (port aynı kalsa bile proxy_port değişmiş olabilir)
        if let Ok(mut url) = state.pac_url.lock() {
            *url = format!("http://{}:{}/proxy.pac", lan_ip, current_port);
        }
        return Ok(PacResponse {
            pac_port: current_port,
        });
    }
    drop(guard); // Lock'u serbest bırak

    // Dinamik PAC port: 8787-8887 arasında müsait olanı bul
    let mut found_port: u16 = 0;
    let mut listener_result = None;
    for port in PAC_PORT_START..=PAC_PORT_END {
        match TcpListener::bind(("0.0.0.0", port)) {
            Ok(l) => {
                found_port = port;
                listener_result = Some(l);
                break;
            }
            Err(_) => continue,
        }
    }
    // Fallback: OS'tan rastgele port iste
    if listener_result.is_none() {
        match TcpListener::bind(("0.0.0.0", 0u16)) {
            Ok(l) => {
                if let Ok(addr) = l.local_addr() {
                    found_port = addr.port();
                }
                listener_result = Some(l);
            }
            Err(e) => return Err(format!("PAC için uygun port bulunamadı: {}", e)),
        }
    }
    let listener = listener_result.unwrap();
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;

    let pac_url = format!("http://{}:{}/proxy.pac", lan_ip, found_port);

    // State'e kaydet
    if let Ok(mut p) = state.pac_port.lock() {
        *p = found_port;
    }
    if let Ok(mut u) = state.pac_url.lock() {
        *u = pac_url.clone();
    }

    let shutdown = Arc::clone(&state.shutdown);
    shutdown.store(false, Ordering::Relaxed);
    let pac_body_arc = Arc::clone(&state.pac_body);
    let pac_url_for_thread = pac_url.clone();

    let join_handle = thread::spawn(move || {
        while !shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let body = Arc::clone(&pac_body_arc);
                    let url = pac_url_for_thread.clone();
                    // Her bağlantıyı ayrı thread'de işle
                    thread::spawn(move || {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        handle_pac_request(stream, &body, &url);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => {}
            }
        }
    });

    let mut guard = state.join_handle.lock().map_err(|e| e.to_string())?;
    *guard = Some(join_handle);
    Ok(PacResponse {
        pac_port: found_port,
    })
}

/// Bağlantı kesildiğinde PAC body'yi DIRECT moduna geçir.
/// Sunucu çalışmaya devam eder — cihazlar internet erişimini kaybetmez.
#[tauri::command]
fn stop_pac_server(state: tauri::State<'_, PacServerState>) -> Result<(), String> {
    // Sunucuyu kapatmak yerine PAC body'yi DIRECT moduna geçir
    if let Ok(mut body) = state.pac_body.lock() {
        *body = make_pac_direct_body();
    }
    Ok(())
}

/// Uygulama tamamen çıkarken PAC sunucusunu gerçekten durdur
fn force_stop_pac_server(state: &PacServerState) {
    // Önce body'yi DIRECT yap (güvenlik için)
    if let Ok(mut body) = state.pac_body.lock() {
        *body = make_pac_direct_body();
    }
    // Sonra shutdown sinyali gönder
    state.shutdown.store(true, Ordering::Relaxed);
    if let Ok(mut guard) = state.join_handle.lock() {
        let _ = guard.take();
    }
}

#[derive(serde::Serialize)]
struct ConfigResponse {
    port: u16,
    lan_ip: String,
    bind_address: String,
}

#[tauri::command]
fn get_sidecar_config(allow_lan_sharing: bool) -> Result<ConfigResponse, String> {
    let bind_addr = if allow_lan_sharing {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };

    // Öncelikli Portlar: 8080 - 8090 arası kontrol et
    let mut selected_port = 0;
    for port in 8080..=8090 {
        if TcpListener::bind((bind_addr, port)).is_ok() {
            selected_port = port;
            break;
        }
    }

    // Fallback: Eğer hepsi doluysa, sistemden rastgele bir port iste (Port 0)
    if selected_port == 0 {
        if let Ok(listener) = TcpListener::bind((bind_addr, 0)) {
            if let Ok(addr) = listener.local_addr() {
                selected_port = addr.port();
            }
        }
    }

    if selected_port == 0 {
        return Err("Uygun port bulunamadı.".to_string());
    }

    // Yerel IP Adresini Bul (LAN Paylaşımı için) — Sanal adaptörleri filtreler
    let lan_ip = get_safe_lan_ip();

    Ok(ConfigResponse {
        port: selected_port,
        lan_ip,
        bind_address: bind_addr.to_string(),
    })
}

/// Registry proxy işlemlerini serialize eden global lock
/// set_system_proxy ve clear_system_proxy eş zamanlı çağrılabilir (reconnect sırasında)
fn proxy_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[tauri::command]
fn clear_system_proxy() -> Result<(), String> {
    let _guard = proxy_lock().lock().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // 1. ProxyEnable = 0
        let status = Command::new("reg")
            .args(&[
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                "/v",
                "ProxyEnable",
                "/t",
                "REG_DWORD",
                "/d",
                "0",
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| e.to_string())?;

        if !status.success() {
            return Err("Failed to clear proxy via registry".to_string());
        }

        // 2. ProxyServer değerini tamamen sil (boş string yaz)
        let _ = Command::new("reg")
            .args(&[
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                "/v",
                "ProxyServer",
                "/t",
                "REG_SZ",
                "/d",
                "",
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        // 3. ProxyOverride değerini de temizle
        let _ = Command::new("reg")
            .args(&[
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                "/v",
                "ProxyOverride",
                "/t",
                "REG_SZ",
                "/d",
                "",
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        // 4. Notify browsers about the change
        notify_proxy_change();
    }
    Ok(())
}

/// Notify Windows that internet settings have changed
/// This forces browsers to immediately pick up the new proxy settings
#[cfg(target_os = "windows")]
fn notify_proxy_change() {
    use std::ptr::null_mut;
    use winapi::um::wininet::{
        INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED, InternetSetOptionW,
    };

    unsafe {
        // Notify that settings have changed
        InternetSetOptionW(null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, null_mut(), 0);
        // Refresh the settings
        InternetSetOptionW(null_mut(), INTERNET_OPTION_REFRESH, null_mut(), 0);
    }
}

#[tauri::command]
fn set_system_proxy(port: u16) -> Result<(), String> {
    let _guard = proxy_lock().lock().map_err(|e| e.to_string())?;
    // ✅ Port aralığı validasyonu
    if port < 1024 {
        return Err("Geçersiz port numarası (1024-65535 arası olmalı)".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let proxy_address = format!("127.0.0.1:{}", port);

        // ✅ Registry yazma iznini kontrol et
        let test_status = Command::new("reg")
            .args(&[
                "query",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Registry erişim hatası: {e}"))?;

        if !test_status.status.success() {
            return Err(
                "Registry yazma izni yok. Uygulamayı yönetici olarak çalıştırın.".to_string(),
            );
        }

        // ✅ ProxyOverride ekle (localhost bypass)
        let _ = Command::new("reg")
            .args(&[
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                "/v",
                "ProxyOverride",
                "/t",
                "REG_SZ",
                "/d",
                "<local>",
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        // 1. Set Proxy Server Address
        let status_server = Command::new("reg")
            .args(&[
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                "/v",
                "ProxyServer",
                "/t",
                "REG_SZ",
                "/d",
                &proxy_address,
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("ProxyServer ayarlanamadı: {e}"))?;

        // 2. Enable Proxy
        let status_enable = Command::new("reg")
            .args(&[
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                "/v",
                "ProxyEnable",
                "/t",
                "REG_DWORD",
                "/d",
                "1",
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("ProxyEnable ayarlanamadı: {e}"))?;

        if !status_server.success() || !status_enable.success() {
            // ✅ Rollback yap
            let _ = Command::new("reg")
                .args(&[
                    "add",
                    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                    "/v",
                    "ProxyEnable",
                    "/t",
                    "REG_DWORD",
                    "/d",
                    "0",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            return Err("Registry güncelleme başarısız, geri alındı.".to_string());
        }

        // 3. CRITICAL: Notify Windows about the change so browsers pick it up immediately
        notify_proxy_change();
    }
    Ok(())
}

#[tauri::command]
fn update_tray_tooltip(app: tauri::AppHandle, tooltip: String) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id("tray") {
        tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn check_port_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

#[tauri::command]
fn check_admin() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // PowerShell ile token elevation kontrolü — net session'dan daha hızlı
        // Domain ortamında ağ çağrısı yapmaz
        let output = std::process::Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-Command",
                "([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();

        if let Ok(out) = output {
            let result = String::from_utf8_lossy(&out.stdout);
            return result.trim().eq_ignore_ascii_case("true");
        }
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

fn perform_app_exit(app: &tauri::AppHandle) {
    let _ = clear_system_proxy();
    std::thread::sleep(std::time::Duration::from_millis(200));
    app.exit(0);
}

/// Uygulama açıldığında eski bypax-proxy süreçlerini temizle (Zombi süreç önleme)
#[tauri::command]
fn kill_zombie_sidecar() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let output = std::process::Command::new("taskkill")
            .args(&["/F", "/IM", "bypax-proxy.exe"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(format!("Zombi süreçler temizlendi: {}", stdout.trim()))
        } else {
            // "not found" normal bir durum — zombi yoktu demek
            Ok(format!(
                "Zombi süreç bulunamadı (normal): {}",
                stderr.trim()
            ))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("Zombi temizleme sadece Windows'ta desteklenir.".to_string())
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    perform_app_exit(&app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PacServerState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri::Manager;
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::TrayIconBuilder;

                let show_i = MenuItem::with_id(app, "show", "Uygulamayı Aç", true, None::<&str>)?;
                let support_i =
                    MenuItem::with_id(app, "support", "Destekle ❤", true, None::<&str>)?;
                let quit_i = MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;

                use tauri::menu::PredefinedMenuItem;
                let s1 = PredefinedMenuItem::separator(app)?;
                let s2 = PredefinedMenuItem::separator(app)?;

                let menu = Menu::with_items(app, &[&show_i, &s1, &support_i, &s2, &quit_i])?;

                // ✅ Debounce için flag
                let is_showing = Arc::new(AtomicBool::new(false));

                let _tray = TrayIconBuilder::with_id("tray")
                    .menu(&menu)
                    .show_menu_on_left_click(false) // ✅ Sol tıkta menü açılmasın, sadece sağ tıkta
                    .icon(app.default_window_icon().unwrap().clone())
                    .tooltip("BypaxDPI - Kapalı")
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.emit("tray_quit", ());
                                let _ = window.close();
                            } else {
                                perform_app_exit(app);
                            }
                        }
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "support" => {
                            use tauri_plugin_opener::OpenerExt;
                            app.opener()
                                .open_url(SUPPORT_URL, None::<&str>)
                                .unwrap_or(());
                        }
                        _ => {}
                    })
                    .on_tray_icon_event({
                        let is_showing = Arc::clone(&is_showing);
                        move |tray, event| {
                            use tauri::tray::{MouseButton, TrayIconEvent};

                            match event {
                                // ✅ Sol tık: pencereyi öne getir
                                TrayIconEvent::Click {
                                    button: MouseButton::Left,
                                    ..
                                } => {
                                    if is_showing.load(Ordering::Relaxed) {
                                        return;
                                    }
                                    is_showing.store(true, Ordering::Relaxed);

                                    let app = tray.app_handle();
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.unminimize();
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }

                                    let is_showing_clone = Arc::clone(&is_showing);
                                    std::thread::spawn(move || {
                                        std::thread::sleep(std::time::Duration::from_millis(300));
                                        is_showing_clone.store(false, Ordering::Relaxed);
                                    });
                                }
                                // ✅ Çift tık: pencereyi öne getir
                                TrayIconEvent::DoubleClick { .. } => {
                                    let app = tray.app_handle();
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.unminimize();
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }
                                }
                                // Sağ tık: menü otomatik açılır
                                _ => {}
                            }
                        }
                    })
                    .build(app)?;

                // LAYER 2: Window close cleanup
                if let Some(window) = app.get_webview_window("main") {
                    window.on_window_event(|event| {
                        if let tauri::WindowEvent::Destroyed = event {
                            let _ = clear_system_proxy();
                        }
                    });
                }
            }
            Ok(())
        })
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // notification plugin zaten yukarıda kayıtlı, tekrar ekleme
        .invoke_handler(tauri::generate_handler![
            clear_system_proxy,
            set_system_proxy,
            update_tray_tooltip,
            check_admin,
            check_port_open,
            get_sidecar_config,
            start_pac_server,
            stop_pac_server,
            kill_zombie_sidecar,
            quit_app
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // LAYER 3: App exit cleanup (fallback)
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let _ = clear_system_proxy();
                if let Some(state) = app_handle.try_state::<PacServerState>() {
                    force_stop_pac_server(&state);
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });
}

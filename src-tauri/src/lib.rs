// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use local_ip_address::list_afinet_netifas;
use std::io::Write;
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use tauri::Emitter;
use tauri::Manager;

// ═══════════════════════════════════════════════════════════════════
// P0-FIX-1: Sentinel dosyası sistemi — crash sonrası proxy kurtarma
// P0-FIX-2: Orijinal proxy ayarları yedekleme / geri yükleme
// ═══════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod registry {
    use winreg::enums::*;
    use winreg::RegKey;

    const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    pub fn read_value_string(name: &str) -> Option<String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu.open_subkey(INTERNET_SETTINGS).ok()?;
        let val: String = key.get_value(name).ok()?;
        Some(val)
    }

    pub fn read_value_dword(name: &str) -> Option<u32> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu.open_subkey(INTERNET_SETTINGS).ok()?;
        key.get_value(name).ok()
    }

    pub fn set_proxy(port: u16) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(INTERNET_SETTINGS)
            .map_err(|e| format!("Registry açılamadı: {}", e))?;

        key.set_value("ProxyServer", &format!("127.0.0.1:{}", port))
            .map_err(|e| format!("ProxyServer: {}", e))?;
        key.set_value("ProxyEnable", &1u32)
            .map_err(|e| format!("ProxyEnable: {}", e))?;
        key.set_value("ProxyOverride", &"<local>")
            .map_err(|e| format!("ProxyOverride: {}", e))?;
        Ok(())
    }

    pub fn clear_proxy() -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(INTERNET_SETTINGS)
            .map_err(|e| format!("Registry açılamadı: {}", e))?;

        key.set_value("ProxyEnable", &0u32)
            .map_err(|e| format!("ProxyEnable: {}", e))?;
        let _ = key.delete_value("ProxyServer");
        let _ = key.delete_value("ProxyOverride");
        Ok(())
    }

    pub fn restore_proxy(
        server: &str,
        enable: u32,
        override_val: Option<&str>,
    ) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(INTERNET_SETTINGS)
            .map_err(|e| format!("Registry açılamadı: {}", e))?;

        key.set_value("ProxyServer", &server)
            .map_err(|e| format!("ProxyServer: {}", e))?;
        key.set_value("ProxyEnable", &enable)
            .map_err(|e| format!("ProxyEnable: {}", e))?;
        if let Some(ov) = override_val {
            key.set_value("ProxyOverride", &ov)
                .map_err(|e| format!("ProxyOverride: {}", e))?;
        }
        Ok(())
    }

    pub fn can_access() -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        hkcu.open_subkey(INTERNET_SETTINGS).is_ok()
    }
}

/// Sentinel dosya yolu — proxy aktifken var, kapanınca silinir.
/// Crash/BSOD/force-kill sonrası hâlâ duruyorsa → dirty shutdown algılanır.
fn sentinel_path() -> std::path::PathBuf {
    std::env::temp_dir().join("bypaxdpi_proxy_active.lock")
}

/// Orijinal proxy ayarlarını tutan yapı
#[derive(Debug, Clone, Default)]
struct OriginalProxySettings {
    proxy_enable: Option<u32>,
    proxy_server: Option<String>,
    proxy_override: Option<String>,
}

/// Orijinal proxy ayarlarını saklayan global state
fn original_proxy_store() -> &'static Mutex<Option<OriginalProxySettings>> {
    static STORE: OnceLock<Mutex<Option<OriginalProxySettings>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(None))
}

/// Proxy ayarlarını set etmeden ÖNCE mevcut değerleri yedekler
#[cfg(target_os = "windows")]
fn backup_proxy_settings() {
    let settings = OriginalProxySettings {
        proxy_enable: registry::read_value_dword("ProxyEnable"),
        proxy_server: registry::read_value_string("ProxyServer"),
        proxy_override: registry::read_value_string("ProxyOverride"),
    };

    if let Ok(mut guard) = original_proxy_store().lock() {
        // Sadece ilk backup'ı al — sonraki set_system_proxy çağrıları üzerine yazmasın
        if guard.is_none() {
            eprintln!("[PROXY-BACKUP] Orijinal ayarlar yedeklendi: {:?}", settings);
            *guard = Some(settings);
        }
    }
}

/// Yedeklenen proxy ayarlarını geri yükler.
/// Eğer orijinal ayarlarda proxy aktifse → geri yükle
/// Eğer orijinal ayarlarda proxy yoksa → sil (mevcut davranış)
#[cfg(target_os = "windows")]
fn restore_proxy_settings() -> bool {
    let original = match original_proxy_store().lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => {
            eprintln!("[WARN] proxy backup lock poisoned, recovering");
            poisoned.into_inner().clone()
        }
    };

    if let Some(orig) = original {
        // Orijinal ProxyServer varsa geri yükle (kurumsal proxy koruması)
        if let Some(ref server) = orig.proxy_server {
            if !server.is_empty() && !server.starts_with("127.0.0.1:") {
                eprintln!("[PROXY-RESTORE] Kurumsal proxy geri yükleniyor: {}", server);

                let enable_val = orig.proxy_enable.unwrap_or(0);
                let _ = registry::restore_proxy(server, enable_val, orig.proxy_override.as_deref());

                return true; // Geri yükleme yapıldı, silme işlemine geçme
            }
        }
    }
    // Orijinal proxy yoktu veya bizimkiyle aynıydı → normal silme prosedürü (mevcut davranış)
    false
}

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
        "tap-",
        "tun",
        "warp",
        "tailscale",
        "zerotier",
        "nordlynx",
        "wireguard",
        "proton",
        "mullvad",
        "windscribe",
        "surfshark",
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

/// Basit string hash — PAC body değişti mi kontrolü için
fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

/// Ön-derlenmiş PAC HTTP yanıtı — her istekte format! çağırmaz
pub struct PacCache {
    pub pac_response: Vec<u8>,
    pub body_hash: u64,
}

/// PAC sunucusu durumu: thread handle + shutdown flag + dinamik body
pub struct PacServerState {
    pub join_handle: Mutex<Option<thread::JoinHandle<()>>>,
    pub shutdown: Arc<AtomicBool>,
    pub pac_body: Arc<Mutex<String>>,
    pub pac_cache: Arc<Mutex<PacCache>>,
    pub pac_port: Mutex<u16>,
    pub pac_url: Mutex<String>,
}

impl Default for PacServerState {
    fn default() -> Self {
        Self {
            join_handle: Mutex::new(None),
            shutdown: Arc::new(AtomicBool::new(false)),
            pac_body: Arc::new(Mutex::new(make_pac_direct_body())),
            pac_cache: Arc::new(Mutex::new(PacCache {
                pac_response: Vec::new(),
                body_hash: 0,
            })),
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
    // Localhost & plain hostnames → DIRECT (anında, DNS yok)
    if (isPlainHostName(host) ||
        host === "localhost" ||
        shExpMatch(host, "127.*") ||
        shExpMatch(host, "10.*") ||
        shExpMatch(host, "192.168.*") ||
        shExpMatch(host, "172.16.*") || shExpMatch(host, "172.17.*") ||
        shExpMatch(host, "172.18.*") || shExpMatch(host, "172.19.*") ||
        shExpMatch(host, "172.2?.*") || shExpMatch(host, "172.30.*") ||
        shExpMatch(host, "172.31.*") ||
        shExpMatch(host, "*.local") ||
        shExpMatch(host, "*.localhost") ||
        shExpMatch(host, "*.internal"))
        return "DIRECT";
    return "PROXY {}; DIRECT";
}}"#,
        proxy
    )
}

fn make_setup_html(pac_url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="tr">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=0">
<title>BypaxDPI – Kurulum</title>
<style>
:root {{
    --bg-color: #09090b;
    --card-bg: #18181b;
    --primary: #3b82f6;
    --primary-hover: #2563eb;
    --success: #22c55e;
    --text-main: #f8fafc;
    --text-muted: #94a3b8;
    --border: rgba(255,255,255,0.08);
}}
* {{ box-sizing: border-box; margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; -webkit-tap-highlight-color: transparent; }}
body {{ background-color: var(--bg-color); color: var(--text-main); line-height: 1.5; padding: 20px 16px; display: flex; flex-direction: column; align-items: center; min-height: 100vh; }}
.container {{ width: 100%; max-width: 440px; display: flex; flex-direction: column; gap: 20px; }}

/* Header */
.header {{ text-align: center; margin-bottom: 10px; animation: fadeDown 0.6s ease; }}
.title {{ font-size: 1.5rem; font-weight: 700; letter-spacing: -0.02em; margin-bottom: 4px; }}
.subtitle {{ font-size: 0.9rem; color: var(--text-muted); }}

/* Card */
.card {{ background: var(--card-bg); border: 1px solid var(--border); border-radius: 20px; padding: 20px; box-shadow: 0 10px 40px rgba(0,0,0,0.5); animation: fadeUp 0.6s ease; }}
.card-title {{ font-size: 1.05rem; font-weight: 600; margin-bottom: 16px; display: flex; align-items: center; gap: 8px; }}

/* Input Group */
.input-group {{ position: relative; margin-bottom: 16px; }}
.url-input {{ width: 100%; background: #27272a; border: 1px solid #3f3f46; color: var(--text-main); font-size: 0.9rem; padding: 14px 16px; border-radius: 12px; outline: none; transition: border-color 0.2s; -webkit-user-select: all; user-select: all; }}
.url-input:focus {{ border-color: var(--primary); }}

/* Copy Button */
.btn-copy {{ width: 100%; height: 50px; background: var(--primary); color: #fff; font-size: 1.05rem; font-weight: 600; padding: 0 20px; border: none; border-radius: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; gap: 8px; transition: all 0.2s; box-shadow: 0 4px 12px rgba(59,130,246,0.3); }}
.btn-copy:active {{ transform: scale(0.98); }}
.btn-copy.success {{ background: var(--success); box-shadow: 0 4px 12px rgba(34,197,94,0.3); }}

/* Guide Button */
.btn-guide {{ display: inline-flex; align-items: center; justify-content: center; background: var(--success); color: #fff; text-decoration: none; padding: 12px 16px; border-radius: 12px; font-size: 0.9rem; font-weight: 600; border: none; width: 100%; margin-top: 12px; transition: all 0.2s; box-shadow: 0 4px 12px rgba(34,197,94,0.3); }}
.btn-guide:active {{ transform: scale(0.98); opacity: 0.9; }}

/* Steps */
.step-list {{ list-style: none; counter-reset: custom-counter; margin-top: 10px; display: flex; flex-direction: column; gap: 12px; }}
.step-item {{ position: relative; padding-left: 36px; font-size: 0.9rem; color: #a1a1aa; }}
.step-item::before {{ content: counter(custom-counter); counter-increment: custom-counter; position: absolute; left: 0; top: -1px; width: 24px; height: 24px; background: rgba(255,255,255,0.1); color: #fff; font-size: 0.75rem; font-weight: 600; display: flex; align-items: center; justify-content: center; border-radius: 50%; }}
.step-item strong {{ color: #e2e8f0; font-weight: 600; display: block; margin-bottom: 2px; }}

/* Language Switcher */
.lang-switcher {{ display: flex; justify-content: center; gap: 12px; margin-bottom: 8px; animation: fadeDown 0.6s ease; }}
.lang-btn {{ background: rgba(255,255,255,0.05); border: 1px solid var(--border); color: #fff; padding: 6px 16px; border-radius: 10px; font-size: 0.85rem; cursor: pointer; transition: all 0.2s; font-weight: 500; }}
.lang-btn.active {{ background: var(--primary); border-color: var(--primary); font-weight: 700; box-shadow: 0 0 15px rgba(59,130,246,0.3); }}

/* Divider */
.divider {{ height: 1px; background: var(--border); margin: 24px 0; }}

/* Animations */
@keyframes fadeUp {{ from {{ opacity: 0; transform: translateY(15px); }} to {{ opacity: 1; transform: translateY(0); }} }}
@keyframes fadeDown {{ from {{ opacity: 0; transform: translateY(-15px); }} to {{ opacity: 1; transform: translateY(0); }} }}

/* Notice */
.notice {{ background: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.2); padding: 12px; border-radius: 12px; margin-top: 20px; font-size: 0.85rem; color: #fca5a5; display: flex; align-items: flex-start; gap: 10px; }}
.notice-icon {{ font-size: 1.2rem; }}
</style>
</head>
<body>
<div class="container">
    <div class="lang-switcher">
        <button class="lang-btn active" id="btn-tr">TÜRKÇE</button>
        <button class="lang-btn" id="btn-en">ENGLISH</button>
    </div>

    <header class="header">
        <h1 class="title" data-tr="BypaxDPI'a Bağlan" data-en="Connect to BypaxDPI">BypaxDPI'a Bağlan</h1>
        <p class="subtitle" data-tr="İnternet trafiğinizi şifreleyin ve engelleri aşın" data-en="Encrypt your traffic and bypass restrictions">İnternet trafiğinizi şifreleyin ve engelleri aşın</p>
    </header>

    <div class="card">
        <div class="card-title">
            <span>📱</span> <span data-tr="Android & iPhone Kurulumu" data-en="Android & iPhone Setup">Android & iPhone Kurulumu</span>
        </div>

        <div class="input-group">
            <input type="text" class="url-input" id="pacurl" value="{}" readonly onclick="this.select();">
        </div>

        <button class="btn-copy" id="copybtn" data-tr="Adresi Kopyala" data-en="Copy Address">
            Adresi Kopyala
        </button>

        <a href="https://bypaxdpi.vercel.app/proxy" target="_blank" class="btn-guide" data-tr="❓ Görsel Kurulum Rehberi" data-en="❓ Visual Setup Guide">
            ❓ Görsel Kurulum Rehberi
        </a>

        <div class="divider"></div>

        <div class="card-title" style="font-size:0.95rem; margin-bottom:12px;" data-tr="Nasıl yapılır kısaca?" data-en="Quick Guide">Nasıl yapılır kısaca?</div>
        <ul class="step-list">
            <li class="step-item">
                <strong data-tr="Yeşil butona basarak adresi kopyalayın." data-en="Copy the address using the green button.">Yeşil butona basarak adresi kopyalayın.</strong>
                <span data-tr="Kopyalanmazsa kutuya uzun basıp elle kopyalayın." data-en="If copy fails, long press the box to copy manually.">Kopyalanmazsa kutuya uzun basıp elle kopyalayın.</span>
            </li>
            <li class="step-item">
                <strong data-tr="Wi-Fi ayarlarınıza gidin." data-en="Go to Wi-Fi settings.">Wi-Fi ayarlarınıza gidin.</strong>
                <span data-tr="Bağlı olduğunuz ağın yanındaki (Ayarlar ⚙️ / i) ikonuna dokunun." data-en="Tap the (Settings ⚙️ / i) icon next to your network.">Bağlı olduğunuz ağın yanındaki (Ayarlar ⚙️ / i) ikonuna dokunun.</span>
            </li>
            <li class="step-item">
                <strong data-tr="Proxy ayarını 'Otomatik / PAC' olarak değiştirin." data-en="Change Proxy to 'Automatic / PAC'.">Proxy ayarını "Otomatik / PAC" olarak değiştirin.</strong>
                <span data-tr="Gelişmiş ayarlar menüsünün altında bulunabilir." data-en="Can be found under advanced settings.">Gelişmiş ayarlar menüsünün altında bulunabilir.</span>
            </li>
            <li class="step-item">
                <strong data-tr="Kopyaladığınız adresi yapıştırın ve kaydedin." data-en="Paste the copied address and save.">Kopyaladığınız adresi yapıştırın ve kaydedin.</strong>
                <span data-tr="Artık bağlantınız güvende!" data-en="Your connection is now secure!">Artık bağlantınız güvende!</span>
            </li>
        </ul>
    </div>

    <div class="notice">
        <span class="notice-icon">⚠</span>
        <div>
            <strong data-tr="ÖNEMLİ:" data-en="IMPORTANT:">ÖNEMLİ:</strong>
            <span data-tr="Uygulamayı kapattıktan sonra telefonunuzda (örn: WhatsApp) internet sorunu yaşarsanız, telefonunuzun Wi-Fi bağlantısını bir kereliğine kapatıp açmanız yeterlidir. (Cache temizlenir)." data-en="If you experience network issues (e.g., WhatsApp) after closing the app, simply toggle your Wi-Fi off and on once. (Clears cache).">Uygulamayı kapattıktan sonra telefonunuzda (örn: WhatsApp) internet sorunu yaşarsanız, telefonunuzun Wi-Fi bağlantısını bir kereliğine kapatıp açmanız yeterlidir. (Cache temizlenir).</span>
        </div>
    </div>
</div>

<script>
(function() {{
    var url = document.getElementById('pacurl').value;
    var btn = document.getElementById('copybtn');
    var currentLang = 'tr';

    function setLanguage(lang) {{
        currentLang = lang;
        document.querySelectorAll('[data-tr]').forEach(function(el) {{
            el.innerHTML = el.getAttribute('data-' + lang);
        }});
        document.getElementById('btn-tr').classList.toggle('active', lang === 'tr');
        document.getElementById('btn-en').classList.toggle('active', lang === 'en');
        
        // Kopyalanmış buton metnini koruyalım eğer o andaysa
        if (btn.classList.contains('success')) {{
             btn.innerHTML = (lang === 'tr' ? '✓ Kopyalandı!' : '✓ Copied!');
        }}
    }}

    document.getElementById('btn-tr').onclick = function() {{ setLanguage('tr'); }};
    document.getElementById('btn-en').onclick = function() {{ setLanguage('en'); }};

    function tryCopy() {{
        if (navigator.clipboard && navigator.clipboard.writeText) {{
            navigator.clipboard.writeText(url).then(function() {{
                showSuccess();
            }}).catch(fallbackCopyTextToClipboard);
        }} else {{
            fallbackCopyTextToClipboard();
        }}
    }}

    function showSuccess() {{
        var originalText = btn.getAttribute('data-' + currentLang);
        btn.innerHTML = (currentLang === 'tr' ? '✓ Kopyalandı!' : '✓ Copied!');
        btn.classList.add('success');
        setTimeout(function() {{
            btn.innerHTML = originalText;
            btn.classList.remove('success');
        }}, 2500);
    }}

    function fallbackCopyTextToClipboard() {{
        var textArea = document.createElement("textarea");
        textArea.value = url;
        textArea.style.top = "0";
        textArea.style.left = "0";
        textArea.style.position = "fixed";
        document.body.appendChild(textArea);
        textArea.focus();
        textArea.select();
        try {{
            var successful = document.execCommand('copy');
            if (successful) showSuccess();
        }} catch (err) {{ }}
        document.body.removeChild(textArea);
    }}

    btn.onclick = tryCopy;
}})();
</script>
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

fn handle_pac_request(
    stream: TcpStream,
    pac_body: &Arc<Mutex<String>>,
    pac_cache: &Arc<Mutex<PacCache>>,
    pac_url: &str,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let mut reader = std::io::BufReader::new(stream);
    let mut first_line = String::new();

    if std::io::BufRead::read_line(&mut reader, &mut first_line).is_err() || first_line.is_empty() {
        return;
    }

    // TCP RST problemini çözmek için request header'larının TAMI tüketilmelidir.
    // Sadece 512 bytelık kısmı okunup soket kapatılırsa OS bağlantıyı "Connection Reset" ile koparır
    // Bu yüzden telefon tarayıcısında QR kodu okutulan setup sayfası hiç açılmamış gözüküyordu.
    let mut discard = String::new();
    while let Ok(n) = std::io::BufRead::read_line(&mut reader, &mut discard) {
        if n <= 2 {
            break;
        } // Boş satır (Header sonu: \r\n veya \n)
        discard.clear();
    }

    let mut stream = reader.into_inner();

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

    if is_get && path == "/proxy.pac" {
        // Body'yi bir kez oku — hem hash hem içerik için kullan (TOCTOU ve deadlock riski önlenir)
        let current_body = pac_body
            .lock()
            .map(|b| b.clone())
            .unwrap_or_else(|_| make_pac_direct_body());
        let current_hash = simple_hash(&current_body);

        if let Ok(mut cache) = pac_cache.lock() {
            if cache.body_hash != current_hash || cache.pac_response.is_empty() {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: max-age=300\r\nContent-Length: {}\r\n\r\n{}",
                    current_body.len(),
                    current_body
                );
                cache.pac_response = response.into_bytes();
                cache.body_hash = current_hash;
            }
            let _ = stream.write_all(&cache.pac_response);
        } else {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: max-age=300\r\nContent-Length: {}\r\n\r\n{}",
                current_body.len(),
                current_body
            );
            let _ = stream.write_all(response.as_bytes());
        }
        let _ = stream.flush();
        return;
    }

    if !is_get {
        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        let _ = stream.flush();
        return;
    }

    let (status, content_type, body) = if path == "/" || path.is_empty() {
        (
            "200 OK",
            "text/html; charset=utf-8",
            make_setup_html(pac_url),
        )
    } else {
        ("404 Not Found", "text/plain", String::new())
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: max-age=300\r\nContent-Length: {}\r\n\r\n{}",
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

/// P1-FIX: PAC sunucusu eşzamanlı bağlantı limiti
const MAX_PAC_CONNECTIONS: u32 = 50;

#[cfg(target_os = "windows")]
fn manage_firewall_rules(enable: bool, proxy_port: u16, pac_port: u16) {
    std::thread::spawn(move || {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Önce mevcut kuralları temizle
        let _ = std::process::Command::new("netsh")
            .args(&[
                "advfirewall",
                "firewall",
                "delete",
                "rule",
                "name=BypaxDPI_Proxy",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        let _ = std::process::Command::new("netsh")
            .args(&[
                "advfirewall",
                "firewall",
                "delete",
                "rule",
                "name=BypaxDPI_PAC",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        if enable {
            let _ = std::process::Command::new("netsh")
                .args(&[
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    "name=BypaxDPI_Proxy",
                    "dir=in",
                    "action=allow",
                    "protocol=TCP",
                    &format!("localport={}", proxy_port),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();

            let _ = std::process::Command::new("netsh")
                .args(&[
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    "name=BypaxDPI_PAC",
                    "dir=in",
                    "action=allow",
                    "protocol=TCP",
                    &format!("localport={}", pac_port),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    });
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

    // P1-FIX: LAN paylaşımı her zaman 0.0.0.0'a bind eder (fonksiyon zaten sadece LAN aktifken çağrılır)
    // Ama yerel cihazların güvenliği için bind adresi sabitlenir
    let bind_addr = "0.0.0.0";

    // Dinamik PAC port: 8787-8887 arasında müsait olanı bul
    let mut found_port: u16 = 0;
    let mut listener_result = None;
    for port in PAC_PORT_START..=PAC_PORT_END {
        match TcpListener::bind((bind_addr, port)) {
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
        match TcpListener::bind((bind_addr, 0u16)) {
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

    #[cfg(target_os = "windows")]
    manage_firewall_rules(true, proxy_port, found_port);

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
    let pac_cache_arc = Arc::clone(&state.pac_cache);
    let pac_url_for_thread = pac_url.clone();

    // P1-FIX: Thread limiti için atomik sayaç
    let active_connections = Arc::new(std::sync::atomic::AtomicU32::new(0));

    let join_handle = thread::spawn(move || {
        while !shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    // P1-FIX: Eşzamanlı bağlantı limiti — DoS koruması
                    let current = active_connections.load(Ordering::Relaxed);
                    if current >= MAX_PAC_CONNECTIONS {
                        // Limit aşıldı — bağlantıyı hemen kapat
                        drop(stream);
                        continue;
                    }
                    active_connections.fetch_add(1, Ordering::Relaxed);

                    let body = Arc::clone(&pac_body_arc);
                    let cache = Arc::clone(&pac_cache_arc);
                    let url = pac_url_for_thread.clone();
                    let conn_counter = Arc::clone(&active_connections);
                    // Her bağlantıyı ayrı thread'de işle
                    thread::spawn(move || {
                        let _ = stream.set_nodelay(true);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        handle_pac_request(stream, &body, &cache, &url);
                        conn_counter.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
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

    #[cfg(target_os = "windows")]
    manage_firewall_rules(false, 0, 0);

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

    #[cfg(target_os = "windows")]
    manage_firewall_rules(false, 0, 0);
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

/// P0-FIX-3: Poisoned mutex recovery — panic sonrası bile proxy temizleme çalışsın
fn acquire_proxy_lock() -> std::sync::MutexGuard<'static, ()> {
    match proxy_lock().lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("[WARN] Proxy lock was poisoned (previous panic?), recovering");
            poisoned.into_inner()
        }
    }
}

#[tauri::command]
fn clear_system_proxy() -> Result<(), String> {
    let _guard = acquire_proxy_lock(); // P0-FIX-3: Poisoned mutex recovery
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // P0-FIX-2: Önce orijinal ayarları geri yüklemeyi dene
        let has_original = restore_proxy_settings();

        if !has_original {
            let _ = registry::clear_proxy();
        }

        // 4. DNS Önbelleğini Temizle (Race condition / DNS sorunlarını önler)
        let _ = Command::new("ipconfig")
            .arg("/flushdns")
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        // 5. Notify browsers about the change
        notify_proxy_change();

        manage_firewall_rules(false, 0, 0);
    }

    // P0-FIX-1: Sentinel dosyasını sil — proxy artık aktif değil
    let _ = std::fs::remove_file(sentinel_path());

    // P0-FIX-2: Backup'ı temizle — geri yükleme tamamlandı
    if let Ok(mut guard) = original_proxy_store().lock() {
        *guard = None;
    }

    Ok(())
}

/// Notify Windows that internet settings have changed
/// This forces browsers to immediately pick up the new proxy settings
#[cfg(target_os = "windows")]
fn notify_proxy_change() {
    use std::ptr::null_mut;
    use winapi::um::wininet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
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
    let _guard = acquire_proxy_lock(); // P0-FIX-3: Poisoned mutex recovery
                                       // ✅ Port aralığı validasyonu
    if port < 1024 {
        return Err("Geçersiz port numarası (1024-65535 arası olmalı)".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        if !registry::can_access() {
            return Err(
                "Registry yazma izni yok. Uygulamayı yönetici olarak çalıştırın.".to_string(),
            );
        }

        // P0-FIX-2: Proxy ayarlamadan ÖNCE mevcut ayarları yedekle
        backup_proxy_settings();

        registry::set_proxy(port).map_err(|e| {
            // Rollback
            let _ = registry::clear_proxy();
            format!("Registry güncelleme başarısız, geri alındı: {}", e)
        })?;

        // 3. CRITICAL: Notify Windows about the change so browsers pick it up immediately
        notify_proxy_change();
    }

    // P0-FIX-1: Sentinel dosyası oluştur — proxy artık aktif
    let _ = std::fs::write(sentinel_path(), format!("port={}", port));

    Ok(())
}

/// P1-FIX: Tooltip uzunluk sınırı — Windows tooltip limiti 128 karakter
#[tauri::command]
fn update_tray_tooltip(app: tauri::AppHandle, tooltip: String) -> Result<(), String> {
    let sanitized: String = tooltip.chars().take(128).collect();
    if let Some(tray) = app.tray_by_id("tray") {
        tray.set_tooltip(Some(sanitized))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// P1-FIX: Port aralığı kısıtlama — XSS ile localhost port taraması engellenir
#[tauri::command]
fn check_port_open(port: u16) -> bool {
    // Sadece privileged portları engelle, dinamik portlara (OS ataması) izin ver
    if port < 1024 {
        return false;
    }
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
        use std::mem;
        use std::ptr;
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
        use winapi::um::securitybaseapi::GetTokenInformation;
        use winapi::um::winnt::{TokenElevation, HANDLE, TOKEN_ELEVATION, TOKEN_QUERY};

        unsafe {
            let mut token: HANDLE = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }

            let mut elevation: TOKEN_ELEVATION = mem::zeroed();
            let mut size: u32 = 0;
            let result = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut _,
                mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            );

            CloseHandle(token);
            result != 0 && elevation.TokenIsElevated != 0
        }
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
fn save_sidecar_pid(pid: u32) {
    let pid_file = std::env::temp_dir().join("bypaxdpi_sidecar.pid");
    let _ = std::fs::write(&pid_file, pid.to_string());
}

/// Uygulama açıldığında eski bypax-proxy süreçlerini temizle (Zombi süreç önleme)
#[tauri::command]
fn kill_zombie_sidecar() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let pid_file = std::env::temp_dir().join("bypaxdpi_sidecar.pid");
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if pid > 0 {
                    let output = std::process::Command::new("taskkill")
                        .args(["/F", "/PID", &pid.to_string()])
                        .creation_flags(CREATE_NO_WINDOW)
                        .output();

                    let _ = std::fs::remove_file(&pid_file);

                    if let Ok(out) = output {
                        if out.status.success() {
                            return Ok(format!("Zombi süreç (PID {}) durduruldu.", pid));
                        }
                    }
                }
            }
        }
        Ok("Zombi PID dosyası bulunamadı.".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("Zombi temizleme sadece Windows'ta desteklenir.".to_string())
    }
}

/// P0-FIX: Ortadaki Adam (Network Reconnaissance) Riskini Engellemek İçin Özel Ping Doğrulayıcı
#[tauri::command]
async fn check_dns_latency(dns_ip: String) -> Result<u32, String> {
    // Sadece bilinen DNS IP'lerini kabul et (Arbitrary internal network scan'i önler)
    let allowed_ips = [
        "1.1.1.1",        // Cloudflare
        "8.8.8.8",        // Google
        "9.9.9.9",        // Quad9
        "94.140.14.14",   // AdGuard
        "208.67.222.222", // OpenDNS
    ];

    if !allowed_ips.contains(&dns_ip.as_str()) {
        return Err("Bilinmeyen DNS adresi".to_string());
    }

    let start = std::time::Instant::now();
    let addr = format!("{}:53", dns_ip)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    match std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(1500)) {
        Ok(_) => Ok(start.elapsed().as_millis() as u32),
        Err(_) => Ok(999),
    }
}

/// P0-FIX-1: Uygulama başlangıcında crash/BSOD sonrası kalan kirli proxy'yi temizle
/// Sentinel dosyası varsa = önceki oturum düzgün kapanmamış demektir
#[tauri::command]
fn startup_proxy_cleanup() -> Result<bool, String> {
    let sentinel = sentinel_path();

    if sentinel.exists() {
        eprintln!("[STARTUP] ⚠️ Dirty shutdown detected — sentinel file found");
        eprintln!("[STARTUP] Cleaning orphaned proxy settings...");

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            use std::process::Command;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            let _ = registry::clear_proxy();

            // DNS cache temizle
            let _ = Command::new("ipconfig")
                .arg("/flushdns")
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            // Tarayıcılara bildir
            notify_proxy_change();
        }

        // Sentinel dosyasını temizle
        let _ = std::fs::remove_file(&sentinel);
        eprintln!("[STARTUP] ✅ Orphaned proxy settings cleaned successfully");

        return Ok(true); // Dirty state temizlendi
    }

    Ok(false) // Temiz başlangıç
}

// 1. Sürücü kontrolü (lib.rs içine ekle)
#[tauri::command]
fn check_driver() -> bool {
    std::path::Path::new("C:\\Windows\\System32\\wpcap.dll").exists()
        || std::path::Path::new("C:\\Windows\\SysWOW64\\wpcap.dll").exists()
}

// 2. Sürücü kurulumu (lib.rs içine ekle)
#[tauri::command]
fn install_driver(app: tauri::AppHandle) -> Result<(), String> {
    let resource_path = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("binaries/npcap-installer.exe");

    if !resource_path.exists() {
        return Err("Sürücü dosyası bulunamadı. Lütfen uygulamayı yeniden yükleyin.".into());
    }

    // P0-FIX: Driver kurulumunu görünür yaptık (/S kaldırıldı, CREATE_NO_WINDOW kaldırıldı)
    // Bu sayede kullanıcı UAC (Yönetici İzni) uyarısını görebilir ve kurulumu tamamlayabilir.
    let status = std::process::Command::new(resource_path)
        .status() // Normal status call, shows window
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err("Kurulum kullanıcı tarafından iptal edildi veya başarısız oldu.".into())
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    perform_app_exit(&app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // P0-FIX: Single-instance enforcement — aynı anda sadece bir BypaxDPI çalışabilir
    #[cfg(target_os = "windows")]
    {
        use std::ptr::null_mut;
        use winapi::shared::winerror::ERROR_ALREADY_EXISTS;
        use winapi::um::errhandlingapi::GetLastError;
        use winapi::um::synchapi::CreateMutexW;

        let mutex_name: Vec<u16> = "Global\\BypaxDPI_SingleInstance\0".encode_utf16().collect();

        unsafe {
            let handle = CreateMutexW(null_mut(), 0, mutex_name.as_ptr());
            if handle.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
                eprintln!("[STARTUP] ❌ BypaxDPI zaten çalışıyor — çıkılıyor");

                // Sessizce çık (Multi-user ortamında diğer kullanıcıları rahatsız etme)
                std::process::exit(0);
            }
            // Windows process sonlandığında mutex handle'ını otomatik temizler
            let _ = handle;
        }
    }

    tauri::Builder::default()
        .manage(PacServerState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::TrayIconBuilder;
                use tauri::Manager;

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
            check_dns_latency,
            save_sidecar_pid,
            startup_proxy_cleanup,
            check_driver,
            install_driver,
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

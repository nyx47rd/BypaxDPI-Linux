// ============================================================
// BypaxDPI — Merkezi Sabitler
// Tüm URL'ler, DNS ayarları ve app sabitleri burada toplanır.
// ============================================================

// ===== Dış Bağlantılar =====
export const URLS = {
  youtube: "https://youtube.com/@ConsolAktif",
  patreon: "https://www.patreon.com/join/ConsolAktif",
  tutorialHowItWorks: "https://bypaxdpi.vercel.app/how-it-works",
  tutorialProxy: "https://bypaxdpi.vercel.app/proxy",
};

// ===== DNS Ayarları =====
export const DNS_MAP = {
  system: null,
  cloudflare: "1.1.1.1",
  adguard: "94.140.14.14",
  google: "8.8.8.8",
  quad9: "9.9.9.9",
  opendns: "208.67.222.222",
};

// DoH (DNS over HTTPS) URL'leri — Port 53 engelleme yapan ISP'lerde bile çalışır
export const DOH_MAP = {
  cloudflare: "https://cloudflare-dns.com/dns-query",
  google: "https://dns.google/dns-query",
  adguard: "https://dns.adguard-dns.com/dns-query",
  quad9: "https://dns.quad9.net/dns-query",
  opendns: "https://doh.opendns.com/dns-query",
};

// ===== Uygulama Sabitleri =====
export const APP = {
  name: "BypaxDPI",
  version: "1.0.0",
  designWidth: 380,
  designHeight: 700,
  maxLogs: 100,
  maxPortRetries: 20,
  maxReconnectAttempts: 5,
  portCheckMaxAttempts: 15,
};

// ===== Retry Gecikmeleri (ms) =====
export const RETRY_DELAYS = [2500, 3000, 6000, 12000, 20000];

// ===== DPI Mod Timeout'ları (ms) =====
export const DPI_TIMEOUTS = {
  "0": 2000, // Turbo
  "1": 2500, // Dengeli
  "2": 4000, // Güçlü
};

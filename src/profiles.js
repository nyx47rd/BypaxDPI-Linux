// ═══════════════════════════════════════════════════════════════════
// profiles.js — Tek Merkezi Yapılandırma Dosyası
// 
// Tüm ISS profilleri, bypass modları, chunk seçenekleri ve
// engine argümanları buradan yönetilir.
// 
// Yeni ISS eklemek veya mevcut ayarları değiştirmek için
// sadece bu dosyayı düzenlemeniz yeterlidir.
// ═══════════════════════════════════════════════════════════════════

// ─── ISS PROFİLLERİ ──────────────────────────────────────────────
// İlk giriş overlay'ı ve Settings → ISS Rehberi'nde kullanılır.
// 
// Her profil:
//   id       → Benzersiz tanımlayıcı
//   mode     → dpiMethod değeri ('0'=Turbo, '1'=Dengeli, '2'=Güçlü)
//   chunk    → httpsChunkSize değeri
//   color    → UI renk kodu
//   bg       → Arkaplan renk kodu (düşük opacity)
//   icon     → İlk giriş overlay emoji ikonu
// ─────────────────────────────────────────────────────────────────

import turknetLogo from './assets/iss-icons/turknet.png';
import milenicomLogo from './assets/iss-icons/milenicom.png';
import turkTelekomLogo from './assets/iss-icons/turktelekom.png';
import vodafoneLogo from './assets/iss-icons/vodafone.png';
import kablonetLogo from './assets/iss-icons/kablonet.png';
import superonlineLogo from './assets/iss-icons/superonline.png';

export const ISP_PROFILES = [
  { 
    id: 'light', 
    mode: '0', 
    chunk: 4, 
    color: '#facc15', 
    bg: 'rgba(250, 204, 21, 0.1)',
    icon: '⚡',
    logos: [turknetLogo],
    // i18n key'leri: issLightName, issLightDesc
  },
  { 
    id: 'mid', 
    mode: '1', 
    chunk: 2, 
    color: '#60a5fa', 
    bg: 'rgba(96, 165, 250, 0.1)',
    icon: '🛡️',
    logos: [],
    // i18n key'leri: issMidName, issMidDesc
  },
  { 
    id: 'heavy', 
    mode: '2', 
    chunk: 1, 
    color: '#60a5fa', 
    bg: 'rgba(96, 165, 250, 0.1)',
    icon: '🔒',
    logos: [kablonetLogo, superonlineLogo,turkTelekomLogo, vodafoneLogo, milenicomLogo],
    // i18n key'leri: issHeavyName, issHeavyDesc
  },
  { 
    id: 'other', 
    mode: '2', 
    chunk: 1, 
    color: '#a78bfa', 
    bg: 'rgba(167, 139, 250, 0.1)',
    icon: '🌐',
    logos: [],
    // i18n key'leri: issOtherName, issOtherDesc
  },
];

// ─── BYPASS MODLARI ──────────────────────────────────────────────
// Settings → Bypass Modu seçicisinde ve engine argüman oluşturucuda kullanılır.
//
// Her mod:
//   id            → dpiMethod değeri ('0', '1', '2')
//   color         → Aktif renk
//   activeBg      → Aktif arkaplan rengi
//   iconBg        → İkon arkaplan rengi
//   iconName      → lucide-react ikon adı
//   hasChunkSize  → Chunk size seçici gösterilsin mi
//   hasNpcap      → Npcap gelişmiş bypass gösterilsin mi
// ─────────────────────────────────────────────────────────────────

export const BYPASS_MODES = [
  {
    id: '0',
    color: '#facc15',
    activeBg: 'rgba(234, 179, 8, 0.1)',
    iconBg: 'rgba(234, 179, 8, 0.2)',
    iconClass: 'yellow',
    iconName: 'Activity',
    hasChunkSize: false,
    hasNpcap: false,
    // i18n key'leri: modeTurboName, modeTurboDesc
  },
  {
    id: '1',
    color: '#4ade80',
    activeBg: 'rgba(34, 197, 94, 0.1)',
    iconBg: 'rgba(34, 197, 94, 0.2)',
    iconClass: 'green',
    iconName: 'Zap',
    hasChunkSize: true,
    hasNpcap: false,
    // i18n key'leri: modeBalancedName, modeBalancedDesc
  },
  {
    id: '2',
    color: '#60a5fa',
    activeBg: 'rgba(59, 130, 246, 0.1)',
    iconBg: 'rgba(59, 130, 246, 0.2)',
    iconClass: 'blue',
    iconName: 'Shield',
    hasChunkSize: true,
    hasNpcap: true,
    // i18n key'leri: modeStrongName, modeStrongDesc
  },
];

// ─── CHUNK SIZE SEÇENEKLERİ ─────────────────────────────────────
// Bypass modlarının chunk size seçicisinde gösterilir.
// Değer dizisi: her mod için gösterilecek chunk size'lar
// ─────────────────────────────────────────────────────────────────

export const CHUNK_SIZES = [
  { value: 1, label: '1' },
  { value: 2, label: '2' },
  { value: 4, label: '4' },
  { value: 8, label: '8' },
];

// ─── VARSAYILAN CHUNK DEĞERLERİ ─────────────────────────────────
// Her mod için varsayılan chunk size
// ─────────────────────────────────────────────────────────────────

export const DEFAULT_CHUNKS = {
  '0': 4,   // Turbo: chunk kullanmaz ama fallback
  '1': 2,   // Dengeli: 2 byte chunk
  '2': 1,   // Güçlü: 1 byte chunk
};

// ─── GEÇERLİ CHUNK DEĞERLERİ ────────────────────────────────────
// Config validasyonunda kullanılır
// ─────────────────────────────────────────────────────────────────

export const VALID_CHUNK_SIZES = [1, 2, 4, 8, 16, 32, 64, 128];

// ─── GEÇERLİ DPI MODLARI ────────────────────────────────────────

export const VALID_DPI_METHODS = ['0', '1', '2'];

// ─── ENGINE ARGÜMAN OLUŞTURUCU ───────────────────────────────────
// SpoofDPI sidecar'ına gönderilecek argümanları oluşturur.
// 
// config   → Mevcut uygulama config'i
// hasDriver → Npcap kurulu mu
// dnsIP    → Seçili DNS IP adresi
// dohUrl   → DoH URL (varsa)
// addLog   → Log fonksiyonu
// t        → Çeviri objesi
// ─────────────────────────────────────────────────────────────────



# 🎉 BypaxDPI v1.0.0 - Stable Release (Enterprise-Ready)

Uzun soluklu geliştirme, sıkı siber güvenlik testleri (Security Audits) ve pürüzsüz mimari iyileştirmelerinin ardından **BypaxDPI v1.0.0 İlk Kararlı Sürümü** yayında! 

Bu sürüm sadece bir arayüz makyajından ibaret değil; arka planda tamamen yeniden yazılmış, çökmelere karşı askeri düzeyde dayanıklı (Anti-Crash) ve kurumsal standartlara sahip devasa bir altyapı yükseltmesidir. Kısıtlamasız internet deneyimi artık daha profesyonel, daha güvenli ve tamamen zahmetsiz.

---

## ✨ Öne Çıkan Epik Yenilikler

### 🛡️ Kurumsal Düzey Çökme Güvenliği (Sentinel Recovery)
Diğer proxy/DPI araçlarının kronik sorunu olan "Elektrik kesilince/uygulama çökünce internetin gitmesi" sorununu tamamen çözdük.
*   **Sentinel Lock (Zombi Avcısı):** BypaxDPI aniden kapansa (BSOD) bile, işletim sistemindeki izlerini akıllıca temizler ve Windows proxy ayarlarının havada kalarak internetinizi bozmasını imkansız kılar.
*   **Original Proxy Backup (Yedekleme):** Bilgisayarınızda zaten bir iş/şirket proxy ayarı varsa, program bunu saygıyla yedeğe alır. İşlemi bittiğinde eski proxy'nizi **birebir geri yükler (Restore).**
*   **Single-Instance Zırhı:** Yanlışlıkla uygulamayı defalarca açıp ağ soketlerini (portları) kilitlemenizi engellemek için kodlanmış "Global Mutex" sayesinde arka planda her zaman tek, kararlı bir BypaxDPI çalışır.

### ⚙️ 3 Kademeli DPI Motoru & Stale Fix
*   **Turbo, Dengeli ve Güçlü Mod:** Hafif kısıtlamalar için ping'i sıfırda tutan *Turbo Mod (SNI)*, inatçı ağlar için paketi parçalayan *Dengeli Mod (Chunk)* ve en sert filtreler için sırayı bozan *Güçlü Mod (Disorder)* anında emrinizde.
*   **DPI Smart-Restart:** Bağlantınız açıkken kopmadan DNS mi değiştirmek istediniz? BypaxDPI artık bağlantıyı tamamen koparmadan "Soft-Restart" atıp yeni DNS (DoH vb.) protokolüne saniyeler içinde canlı (Live) uyum sağlıyor.

### 🌐 LAN (Yerel Ağ) Paylaşımı & Asenkron PAC Sunucusu
*   **Tüm Evi Özgürleştirin:** Bilgisayarınızdaki BypaxDPI bağlantısını, ağdaki telefon, tablet ve konsollarınıza bir QR Kod veya link üzerinden saniyeler içinde paylaştırın. Evdeki hiçbir cihaza ek program kurmanıza gerek kalmaz.
*   **Rate-Limiting (DoS Koruması):** Ağdaki diğer cihazlar açık PAC portuna birden yüklenmesin diye *Asenkron Thread Limitleyici* kodlanarak bellek şişmesi (OOM) ve lag riskleri sıfırlandı.

### ⚡️ Yeni Akıllı DNS Sistemi
*   **Canlı Gecikme Ölçümü (Live Ping):** DNS sunucularını anlık olarak tarar ve (ms) gecikmelerini ekranda canlı oynatır.
*   **Dinamik Sıralama:** Listede yer alan sunucuları, o anki test performanslarına göre gerçek zamanlı yeniden konumlandırıp en hızlısını hep göz önünde tutar. Ekranda donma veya menü takılması yaratmaz.

### 🎨 Fluent UX, Telemetri Sıfır & Güvenlik Zirvesi
*   **Premium Fluent Arayüz:** Windows ile bütünleşik "Glassmorphism" tasarıma, kusursuz geçiş animasyonlarına ve Canlı Log ekolayzerine sahiptir.
*   **XSS & RCE İzolasyonu:** Tauri v2 Capabilities üzerinden uzaktan kod çalıştırma yolları mühürlenirken, arayüzde çalışan sıkı *DOMPurify Regexleri* Javascript ve CSS enjeksiyonu yapılmasını imkansız hale getirir.
*   **%100 Sıfır Log Şeffaflığı:** Asla bir sunucuyla kullanıcı alışkanlığı, IP veya girdiğiniz sitenin verisi paylaşılmaz. BypaxDPI tamamen offline prensiplerle tasarlanmış mutlak gizlilik aracıdır.

---

## 📦 Kurulum ve Çalıştırma

1. Hemen aşağıdan **`BypaxDPI_1.0.0_x64-setup.exe`** dosyasını indirin.
2. Basit Setup adımlarını tamamlayın (WinPcap gibi ekstra harici sürücü kurulumu asla istemez - Motor içindedir).
3. Uygulamayı çalıştırın, Modunuzu/DNS'inizi seçin ve **BAĞLAN** düğmesine basarak özgür, şeffaf bir dünyaya adım atın!

---

💬 **Sizlerden Gelecekler Gücümüzdür:** Projenin bir topluluk eseri olarak büyümesi için desteğinizi esirgemeyin. Yıldız bırakmayı (Star) veya [Issues](https://github.com/MuratGuelr/BypaxDPI-Windows/issues) sekmesinden fikirlerinizi yeşertmeyi unutmayın.

<div align="center">
  <strong>🔥 Sınırları Aşan İnternet Deneyimine Hoş Geldiniz!</strong>
</div>

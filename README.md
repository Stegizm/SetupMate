# SetupMate

Linux'a yeni başlayanlar için **güvenli**, GUI tabanlı bir kurulum rehberi.
SetupMate tek tuşla her şeyi kuran bir araç değildir: dağıtım ailesini, GPU
üreticisini, cihaz tipini ve oturumu tespit eder; katalogdan gelen doğrulanmış
önerileri gösterir. Kurulum yalnızca kullanıcı planı ve ayrı onay penceresini
açıkça onayladığında çalışır.

## Güvenlik ilkeleri

- Plan her zaman önce **önizleme** olarak gösterilir; hiçbir komut kendiliğinden
  çalışmaz.
- Kurulum, ayrı bir **onay penceresinde** listelenen komutların kullanıcı
  tarafından onaylanmasından sonra başlar. Onay yoksa runner hiç çağrılmaz.
- Paket adı uydurulmaz. Kaynağı doğrulanmayan adlar `TODO_VERIFY` olarak tutulur
  ve plana alınmaz; bu kayıtlar için komut üretilmez.
- `verified` bir kayıt `source` alanı olmadan geçerli sayılmaz.
- Kapsam dışı: paket kaldırma, repo/anahtar ekleme, bootloader/fstab/systemd
  değişikliği, kernel modülü yükleme, config/dotfiles otomatik değiştirme.
- GPU sürücü paketleri yüksek risklidir; yalnızca rehber olarak gösterilir.
- Yönetici yetkisi `pkexec` ile istenir. `pkexec` yoksa uygulama komut çalıştırmaz;
  planı kopyalayıp terminalde `sudo` ile çalıştırmanı önerir.

## Kurulum

### AppImage (önerilen)

1. [Releases](https://github.com/stegizm/SetupMate/releases) sayfasından
   `SetupMate-<sürüm>-x86_64.AppImage` ve `SHA256SUMS` dosyalarını indir.
2. İndirilen dosyayı çalıştırılabilir yap ve başlat:

```bash
chmod +x SetupMate-*.AppImage
./SetupMate-*.AppImage
```

AppImage kurulumu kendini güncelleyebilir:

```bash
./SetupMate-*.AppImage --check-update   # yeni sürüm var mı?
./SetupMate-*.AppImage --update         # AppImage'i son sürüme değiştir
```

Güncelleme, GitHub release'indeki `SHA256SUMS` ile doğrulanır ve yalnızca
`$APPIMAGE` yolundaki dosya atomik olarak değiştirilir.

### Arch Linux (AUR)

`packaging/aur/PKGBUILD` bir **kaynak** paketidir; AppImage kullanmaz. AUR'a
yayınlandıktan sonra:

```bash
yay -S setupmate
# veya
paru -S setupmate
```

AUR/paket yöneticisi kurulumları kendini güncellemez; `yay -Syu` (veya
dağıtımının güncelleme komutu) kullanılır. `setupmate --update` bu kurulumlarda
`NotAppImage` hatası verir ve paket yöneticisine yönlendirir.

### Kaynaktan

```bash
. "$HOME/.cargo/env"
cargo build --release -p setupmate-gui
./target/release/setupmate-gui
```

## Kullanım

1. **Sistemi tanı** — dağıtım ailesi, GPU, cihaz ve oturum salt-okunur tespit edilir.
2. **Amacını seç** — kategori, "Önerilen" veya "Eksikler" görünümüyle listeyi daralt.
3. **Paketleri işaretle** — yalnızca işaretlediğin doğrulanmış paketler plana girer.
4. **Onayla ve kur** — "Kurulumu başlat" onay penceresini açar; komutları görüp
   "Onayla ve kur" dediğinde kurulum arka planda çalışır ve günlük gösterilir.

`TODO_VERIFY` satırları listede görünür ama işaretlenemez ve plana hiçbir zaman
girmez.

## Durum

- Çekirdek (`setupmate-core`): tespit, katalog, planlayıcı, executor, raporlama,
  SHA-256 ve kendi kendini güncelleme.
- GUI (`setupmate-gui`): eframe/egui 0.37, koyu lacivert + teal dashboard,
  onaylı canlı kurulum, ilerleme günlüğü ve terminal CLI modları.
- `data/catalog/packages.toml` **altı dağıtım ailesi** için doğrulanmış kayıtlar
  içerir (debian/apt, fedora/dnf, arch/pacman, opensuse/zypper, alpine/apk,
  void/xbps). Kategoriler: Temel Araçlar, Geliştirme, Oyun, Multimedya, Ofis, GPU,
  Yedekleme & Senkron, Gizlilik & Güvenlik, Terminal Yardımcıları, Ağ Araçları,
  Ses, Ekran Görüntüsü, Konteyner. Aynı araç farklı ailelerde farklı paket adı
  taşıyabilir (ör. `fd` → Debian/Fedora `fd-find`, GNU Privacy Guard →
  Fedora `gnupg2`, openSUSE `gpg2`). Adlar resmi kaynaklardan doğrulanır.
- Katalog ikiliye gömülüdür (`include_str!`); AppImage/AUR kurulumu `data/`
  dizinine ihtiyaç duymaz. Geliştirme için `SETUPMATE_CATALOG_DIR` ile dizin
  geçersiz kılınabilir.
- Türev dağıtımlar da tanınır: Mint/Pop!_OS/Kali → Debian, Nobara/AlmaLinux/Rocky →
  Fedora, Manjaro/EndeavourOS/CachyOS → Arch, Leap/Tumbleweed → openSUSE,
  postmarketOS → Alpine.
- Kurulu paket tespiti salt-okunur: Debian `dpkg`, Arch pacman, Alpine `apk`,
  Void `xbps` ve Fedora/openSUSE `rpmdb.sqlite` (read-only). Veritabanı okunamazsa
  sonuç `Unknown` olur; tahmin edilmez.
- GUI'de paket seçimi (tik), kapsam çipleri, profil hazır setleri, sıralama/risk
  filtresi, tıklanabilir kaynak bağlantıları ve **planı kopyala / dosyaya kaydet**
  dışa aktarma vardır.

## Gereksinimler

- Linux (Wayland veya X11)
- Rust (rustup ile kurulmuş) — yalnızca kaynaktan derleme için

Yeni bir kabukta ortamı yükle:

```bash
. "$HOME/.cargo/env"
```

## Komutlar

```bash
cargo check --workspace
cargo test --workspace
cargo run -p setupmate-gui
```

Kalite kontrolleri (CI ile aynı):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Tek test çalıştırmak için:

```bash
cargo test -p setupmate-core <test_name>
```

## Paketleme

```bash
# AppImage (dist/ altına yazar; appimagetool gerekirse indirilir)
./packaging/build-appimage.sh

# AUR kaynak paketi
cd packaging/aur && makepkg -si
```

Sürüm etiketi (`v0.2.0`) push edildiğinde `.github/workflows/release.yml`
AppImage'i derler, `SHA256SUMS` üretir ve GitHub release'ine yükler.

## Proje yapısı

```text
crates/setupmate-core/   GUI'den bağımsız kütüphane
  models                 veri tipleri (distro, GPU, risk, doğrulama)
  detection              salt-okunur tespit (/etc/os-release, sysfs, oturum)
  installed              kurulu paket tespiti (dpkg / pacman / apk / xbps / rpm sqlite)
  catalog                gömülü TOML katalog + doğrulama
  package_managers       distro -> yönetici eşlemesi ve komut üretimi
  planner                önizleme planı + canlı (non-interactive) plan
  executor               CommandRunner, SystemRunner, Approval kapısı
  reporting              Türkçe plan/rapor metni
  sha256                 bağımlılıksız SHA-256
  sys                    PATH/root/pkexec yardımcıları
  updater                GitHub release tabanlı AppImage güncellemesi
crates/setupmate-gui/    eframe/egui 0.37 arayüz + CLI modları
data/catalog/            categories / packages / profiles TOML dosyaları
packaging/               AppImage betiği, desktop, SVG, AUR PKGBUILD
docs/ai/                 karar günlüğü ve doğrulanmamış adlar
.github/workflows/       CI (fmt + clippy + test) ve Release
```

## Kataloğa paket ekleme

1. Paket adını dağıtımın resmi belgelerinden/paket sayfasından doğrula.
2. `data/catalog/packages.toml` içine kaydı `source` alanıyla ekle.
3. `verification_status = "verified"` kullan; emin değilsen `TODO_VERIFY` bırak.
4. `cargo test -p setupmate-core` ile katalog doğrulamasını çalıştır.
5. GUI'de "Yeniden tespit et" ile kataloğu yenile.

## Notlar

- `docs/ai/decisions.md` kritik kararları, `docs/ai/todo-package-names.md`
  doğrulanmamış adları tutar.
- `opencode.json` düz metin API anahtarı içerebilir; `.gitignore` ile hariç
  tutulur, sürüm kontrolüne eklenmemelidir.

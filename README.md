<div align="center">

# SetupMate

**Linux'a yeni başlayanlar için güvenli, onaylı kurulum rehberi.**

Dağıtımını, GPU'nu ve oturumunu tanır; katalogdan doğrulanmış paketleri önerir.
Ne kuracağını **önce gösterir**, kurulumu ancak sen onayladığında yapar.

[![CI](https://github.com/Stegizm/SetupMate/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Stegizm/SetupMate/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Stegizm/SetupMate?label=release)](https://github.com/Stegizm/SetupMate/releases/latest)
[![License](https://img.shields.io/github/license/Stegizm/SetupMate)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux-2dd4bf)](#desteklenen-dağıtımlar)

</div>

---

## İçindekiler

- [Neden SetupMate?](#neden-setupmate)
- [Güvenlik modeli](#güvenlik-modeli)
- [Kurulum](#kurulum)
- [Güncelleme](#güncelleme)
- [Kullanım](#kullanım)
- [Komut satırı](#komut-satırı)
- [Desteklenen dağıtımlar](#desteklenen-dağıtımlar)
- [Katalog ve doğrulama politikası](#katalog-ve-doğrulama-politikası)
- [Geliştirme](#geliştirme)
- [Paketleme ve yayınlama](#paketleme-ve-yayınlama)
- [Yol haritası](#yol-haritası)
- [Lisans](#lisans)

## Neden SetupMate?

Yeni bir Linux kurulumunda en zor kısım komutlar değil, **ne kurulacağına karar
vermektir**. SetupMate bu kararı senin yerine vermez; kararı verebilmen için
gereken bilgiyi tek ekranda toplar.

- **Sistemini tanır.** Dağıtım ailesi (türevler dahil), GPU üreticisi, dizüstü/masaüstü
  ve Wayland/X11 oturumu salt-okunur yöntemlerle tespit edilir.
- **Paket adı uydurmaz.** Her ad `data/catalog/packages.toml` içinden gelir ve resmî
  kaynağıyla birlikte saklanır. Doğrulanmamış adlar `TODO_VERIFY` kalır ve asla
  kurulum planına girmez.
- **Önce gösterir.** Plan; paket adı, paket yöneticisi, risk seviyesi ve kaynak
  bağlantısıyla listelenir. Kopyalanabilir, dosyaya kaydedilebilir.
- **Onayla kurar.** Kurulum ayrı bir onay penceresinde listelenen komutlarla,
  `pkexec` üzerinden yönetici yetkisiyle çalışır. Onay yoksa hiçbir şey çalışmaz.
- **Kendini günceller.** AppImage kurulumları GitHub release'lerinden tek komutla
  güncellenir; indirilen dosya SHA-256 ile doğrulanır.

## Güvenlik modeli

```text
Tespit (salt-okunur)  →  Öneri  →  Seçim  →  Onay penceresi  →  Kurulum
        │                    │          │             │
   /etc/os-release      katalog     kullanıcı     tam komut listesi
     sysfs, XDG                       tikleri      + risk özeti
```

1. **Tespit** — yalnızca okuma yapılır. Harici komut çalıştırılmaz.
2. **Öneri** — katalogdan, tespit edilen dağıtım ailesine ait doğrulanmış kayıtlar gelir.
3. **Seçim** — plan **yalnızca** işaretlediğin paketlerden kurulur.
4. **Onay** — "Kurulumu başlat" düğmesi sadece onay penceresini açar; pencere
   çalıştırılacak tam komutları listeler.
5. **Kurulum** — ilk hatada durur, günlük ekranda görünür, bitince kurulu durum
   yeniden okunur.

**SetupMate bu sürümde asla:**

- paket kaldırmaz,
- depo, PPA, RPM Fusion veya imzalama anahtarı eklemez,
- bootloader, `fstab`, systemd birimi veya çekirdek modülü değiştirmez,
- yapılandırma dosyalarını ya da dotfiles'ı ezmez,
- GPU sürücüsü kurmaz — sürücüler yalnızca **rehber** olarak gösterilir.

Yönetici yetkisi `pkexec` ile istenir. `pkexec` yoksa uygulama komut çalıştırmaz;
planı kopyalayıp terminalde `sudo` ile çalıştırman için yönlendirme gösterir.

## Kurulum

### AppImage (önerilen)

```bash
# Releases sayfasından indir
curl -fsSLO https://github.com/Stegizm/SetupMate/releases/latest/download/SetupMate-0.1.0-x86_64.AppImage
curl -fsSLO https://github.com/Stegizm/SetupMate/releases/latest/download/SHA256SUMS

# Doğrula ve çalıştır
sha256sum -c SHA256SUMS
chmod +x SetupMate-0.1.0-x86_64.AppImage
./SetupMate-0.1.0-x86_64.AppImage
```

AppImage tek dosyadır; kurulum gerektirmez ve `data/` dizinine ihtiyaç duymaz
(katalog ikiliye gömülüdür).

### Arch Linux (AUR)

```bash
yay -S setupmate
# veya
paru -S setupmate
```

AUR paketi kaynaktan derlenir ve `/usr/bin/setupmate` olarak kurulur. Paket
yöneticisi kurulumları kendini güncellemez; `yay -Syu` kullanılır.

### Kaynaktan

```bash
. "$HOME/.cargo/env"
git clone https://github.com/Stegizm/SetupMate.git
cd SetupMate
cargo build --release -p setupmate-gui
./target/release/setupmate-gui
```

## Güncelleme

AppImage kurulumları:

```bash
./SetupMate-0.1.0-x86_64.AppImage --check-update   # yeni sürüm var mı?
./SetupMate-0.1.0-x86_64.AppImage --update         # son sürüme geç
```

`--update` yalnızca `$APPIMAGE` yolundaki dosyayı değiştirir: yeni AppImage
indirilir, `SHA256SUMS` ile doğrulanır ve atomik olarak yerine konur. AUR veya
paket yöneticisi kurulumlarında `NotAppImage` mesajı verilir ve `yay -Syu`
yönlendirmesi yapılır.

## Kullanım

| Adım | Ne yaparsın | Ne olur |
|:--:|---|---|
| 1 | **Sistemi tanı** | Dağıtım ailesi, GPU, cihaz tipi ve oturum salt-okunur tespit edilir. |
| 2 | **Amacını seç** | "Tümü", "Önerilen" veya "Eksikler" görünümüyle liste daralır. |
| 3 | **Paketleri işaretle** | Yalnızca işaretlediğin doğrulanmış paketler plana girer. |
| 4 | **Onayla ve kur** | Onay penceresindeki komutları görürsün; onaylarsan kurulum arka planda çalışır. |

`TODO_VERIFY` satırları listede görünür ama işaretlenemez ve plana hiçbir zaman
girmez. Paket satırlarındaki `kurulu` / `eksik` rozetleri yerel paket
veritabanından okunur; veritabanı okunamıyorsa sonuç **tahmin edilmez**, açıkça
"bilinmiyor" denir.

## Komut satırı

| Komut | Açıklama |
|---|---|
| `setupmate` | Grafik arayüzü açar. |
| `setupmate --check-update` | Yeni sürüm olup olmadığını denetler. |
| `setupmate --update` | AppImage kurulumunu son sürüme günceller. |
| `setupmate --version` | Sürümü yazar. |
| `setupmate --help` | Yardımı gösterir. |

## Desteklenen dağıtımlar

| Aile | Paket yöneticisi | Kurulu paket tespiti | Türev örnekleri |
|---|---|---|---|
| Debian | `apt-get` | dpkg durum dosyası | Ubuntu, Mint, Pop!\_OS, Kali |
| Fedora | `dnf` | `rpmdb.sqlite` (salt-okunur) | Nobara, AlmaLinux, Rocky |
| Arch | `pacman` | `local/` veritabanı | Manjaro, EndeavourOS, CachyOS |
| openSUSE | `zypper` | `rpmdb.sqlite` (salt-okunur) | Leap, Tumbleweed |
| Alpine | `apk` | `/lib/apk/db/installed` | postmarketOS |
| Void | `xbps-install` | `pkgdb-*.plist` | — |

Dağıtım ailesi `ID` ve `ID_LIKE` alanlarından çıkarılır; türevler taban
dağıtıma eşlenir. Aynı araç farklı ailelerde farklı ad taşıyabilir
(ör. `fd` → Debian/Fedora `fd-find`, GNU Privacy Guard → Fedora `gnupg2`,
openSUSE `gpg2`).

## Katalog ve doğrulama politikası

Katalog üç TOML dosyasından oluşur ve **ikiliye gömülüdür**
(`Catalog::load_embedded`), böylece AppImage ve AUR kurulumları `data/` dizinine
ihtiyaç duymaz. Geliştirme sırasında `SETUPMATE_CATALOG_DIR` ile dizin geçersiz
kılınabilir.

| Durum | Anlamı | Plana girer mi? |
|---|---|:--:|
| `verified` | Ad resmî kaynaktan doğrulandı, `source` alanı dolu | ✅ |
| `TODO_VERIFY` | Ad doğrulanmadı veya dağıtıma özgü | ❌ |
| `risk = high` | Sürücü/çekirdek gibi yüksek riskli işler | ❌ (yalnızca rehber) |

Yeni paket eklemek için:

1. Adı dağıtımın resmî belgelerinden veya paket sayfasından doğrula.
2. `data/catalog/packages.toml` içine `source` alanıyla ekle.
3. `verification_status = "verified"` kullan; emin değilsen `TODO_VERIFY` bırak.
4. `cargo test -p setupmate-core` ile katalog doğrulamasını çalıştır.

Doğrulanmamış adlar `docs/ai/todo-package-names.md` içinde tutulur.

## Geliştirme

```bash
. "$HOME/.cargo/env"

cargo check --workspace
cargo test --workspace
cargo run -p setupmate-gui
```

Kalite kapıları (CI ile aynı):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Proje yapısı

```text
crates/setupmate-core/     GUI'den bağımsız kütüphane
  models/                  veri tipleri (dağıtım, GPU, risk, doğrulama)
  detection/               salt-okunur tespit (/etc/os-release, sysfs, oturum)
  installed/               kurulu paket tespiti (dpkg, pacman, apk, xbps, rpm sqlite)
  catalog/                 gömülü TOML katalog + tutarlılık doğrulaması
  package_managers/        dağıtım → yönetici eşlemesi ve komut üretimi
  planner/                 önizleme planı + canlı (non-interactive) plan
  executor/                CommandRunner, SystemRunner, Approval kapısı
  reporting/               Türkçe plan/rapor metni
  sha256/                  bağımlılıksız SHA-256
  sys/                     PATH, root ve pkexec yardımcıları
  updater/                 GitHub release tabanlı AppImage güncellemesi
crates/setupmate-gui/      eframe/egui 0.37 arayüz + CLI modları
data/catalog/              categories / packages / profiles TOML
packaging/                 AppImage betiği, desktop, SVG, AUR PKGBUILD
scripts/deploy.sh          push + etiket + release doğrulama
docs/ai/                   karar günlüğü ve doğrulanmamış adlar
.github/workflows/         CI (fmt + clippy + test) ve Release
```

Kütüphane kodu `Result` döner ve paniklemez; `unwrap`/`expect` yalnızca testlerde
kullanılır. Harici bağımlılık eklemek onay gerektirir.

## Paketleme ve yayınlama

```bash
# AppImage → dist/SetupMate-<sürüm>-<arch>.AppImage + SHA256SUMS
./packaging/build-appimage.sh

# AUR kaynak paketi
cd packaging/aur && makepkg -si
```

`v*` biçiminde bir etiket push edildiğinde `.github/workflows/release.yml`
AppImage'i derler, `SHA256SUMS` üretir ve GitHub release'ine yükler. Varlık adı
`SetupMate-<sürüm>-<arch>.AppImage` biçiminde kalmalıdır; güncelleyici sürümü bu
addan okur. Ayrıntılı adımlar: [`docs/deploy.md`](docs/deploy.md).

## Yol haritası

- [x] Salt-okunur sistem tespiti ve türev dağıtım eşlemesi
- [x] Altı dağıtım ailesi için doğrulanmış katalog
- [x] Önizleme planı, paket seçimi ve dışa aktarma
- [x] Onay pencereli canlı kurulum + ilerleme günlüğü
- [x] AppImage, AUR ve kendi kendini güncelleme
- [ ] Katalog kapsamını genişletme
- [ ] Kurulum sonrası doğrulama raporu
- [ ] Ek depo gerektiren araçlar için rehber akışları

## Lisans

[MIT](LICENSE)

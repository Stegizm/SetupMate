# Yayınlama adımları

Bu dosya, SetupMate'i GitHub'a ve AUR'a yayınlarken izlenecek adımları özetler.
Komutlar `Stegizm/SetupMate` deposu için yazılmıştır.

## 1. GitHub deposunu hazırla

Yerel depo hazır: `main` dalı `origin`'e bağlı. Kalan tek adım push:

```bash
cd /home/ayaz/Masaüstü/projeler/SetupMate
git push -u origin main
```

> `opencode.json` `.gitignore` içinde. API anahtarı içerdiği için commit edilmemeli.
> `dist/` ve `target/` de hariç tutulur.

## 2. AppImage sürümünü yayınla

Sürüm etiketi push edildiğinde `.github/workflows/release.yml` çalışır:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Workflow şunları üretir ve release'e ekler:

- `SetupMate-0.1.0-x86_64.AppImage`
- `SHA256SUMS`

Release tamamlandıktan sonra güncellemeyi doğrula:

```bash
./SetupMate-0.1.0-x86_64.AppImage --check-update
./SetupMate-0.1.0-x86_64.AppImage --update
```

## 3. AUR paketi

`packaging/aur/PKGBUILD` kaynak paketidir. Etiket yayınlandıktan sonra tarball
özetini yaz ve doğrula:

```bash
cd packaging/aur
updpkgsums
makepkg --printsrcinfo > .SRCINFO
makepkg --verifysource
```

Ardından AUR deposuna gönder (AUR hesabına SSH anahtarı ekli olmalı):

```bash
git clone ssh://aur@aur.archlinux.org/setupmate.git /tmp/aur-setupmate
cp PKGBUILD .SRCINFO /tmp/aur-setupmate/
cd /tmp/aur-setupmate
git add PKGBUILD .SRCINFO
git commit -m "setupmate 0.1.0"
git push
```

AUR kurulumu AppImage kullanmaz; `setupmate --update` bu kurulumlarda
`NotAppImage` mesajı verir ve `yay -Syu` / `paru -Syu` yönlendirmesi yapar.

## 4. Sürüm yükseltirken

1. `crates/setupmate-core/Cargo.toml`, `crates/setupmate-gui/Cargo.toml` ve
   GUI'deki `setupmate-core` bağımlılık sürümünü birlikte artır.
2. `Cargo.lock`'u `cargo check --workspace` ile güncelle.
3. `packaging/aur/PKGBUILD` ve `.SRCINFO` içindeki `pkgver`'i güncelle.
4. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace` çalıştır.
5. Yeni `vX.Y.Z` etiketini push et, ardından AUR özetini yenile ve AUR adımlarını
   yinele.

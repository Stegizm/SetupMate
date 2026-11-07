# Doğrulanmamış Paket Adları (TODO_VERIFY)

Katalog (`data/catalog/`) artık **altı dağıtım ailesi** için doğrulanmış kayıtlar
içerir: debian/apt, fedora/dnf, arch/pacman, opensuse/zypper, alpine/apk, void/xbps.

Kurallar:

- Paket adı uydurulmaz.
- Kaynağı doğrulanamayan, distroya özel, sürücüyle ilgili veya repo/AUR/RPM Fusion/PPA/Flatpak remote gerektiren adlar `TODO_VERIFY` olarak işaretlenir.
- `TODO_VERIFY` olan paketler install planına alınmaz ve install komutu üretilmez.
- `verification_status = "verified"` olan bir kayıt **boş `source` ile geçerli değildir**; `Catalog::validate` bunu reddeder.
- Her kayıt şu alanlarla tutulur: `catalog_id`, `family`, `manager`, `package_or_TODO_VERIFY`, `risk`, `verification_status`, `reason`, `source`.

## Doğrulama kaynakları

- debian → `https://packages.debian.org/stable/` (stable suite) / `api.ftp-master.debian.org`
- fedora → `https://mdapi.fedoraproject.org/f44/` (binary package)
- arch → `https://archlinux.org/packages/` (repo + paket sayfası)
- opensuse → `https://download.opensuse.org/tumbleweed/repo/oss/x86_64/` (+ `/noarch/`)
- alpine → `https://dl-cdn.alpinelinux.org/alpine/edge/{main,community}/x86_64/APKINDEX.tar.gz`
  (doğrulama APKINDEX `P:` satırlarından; katalog `source` alanı resmi paket arama sayfası)
- void → `https://repo-default.voidlinux.org/current/x86_64-repodata` (zstd + tar → `index.plist`;
  katalog `source` alanı resmi paket arama sayfası)

## Doğrulanmış kayıtlar

Toplam **353 kayıt** (347 doğrulanmış + 6 GPU rehber):

- **basics:** curl, wget, htop, btop, tree, unzip, zip, man-db, ncdu, jq, nano, fastfetch (71 kayıt)
- **development:** git, neovim, vim, tmux, ripgrep, fd, fzf, bat, gcc, make, clang, base-devel (65)
- **gaming:** gamemode, mangohud, gamescope, vkbasalt, goverlay, lutris, prismlauncher, wine (26; yalnızca 4 masaüstü ailesi)
- **multimedia:** vlc, mpv, ffmpeg, gimp, obs-studio (27)
- **office:** libreoffice, thunderbird, hunspell, okular (23)
- **backup:** rsync, restic, rclone (18)
- **privacy:** keepassxc, gnupg, openssh (18)
- **terminal:** zoxide, eza, git-delta, alacritty, kitty (28)
- **network:** whois, mtr, traceroute, nload (24)
- **audio:** audacity, pavucontrol, easyeffects (18)
- **capture:** flameshot, scrot, grim (18)
- **containers:** podman, distrobox (11)
- **gpu:** nvidia-driver (6; yalnızca rehber; `TODO_VERIFY` + `high`)

Aileye göre farklı adlar:

- `fd` → debian/fedora `fd-find`, arch/alpine/void `fd`, opensuse `fd`
- `man-db` → debian/fedora/arch/alpine/void `man-db`, opensuse `man`
- `vim` → fedora `vim-enhanced` (diğerleri `vim`)
- `libreoffice` → arch `libreoffice-fresh` (diğerleri `libreoffice`)
- `gnupg` → debian/arch/alpine/void `gnupg`, fedora `gnupg2`, opensuse `gpg2`
- `openssh` → debian `openssh-client` (diğerleri `openssh`)

## TODO_VERIFY kayıtları

- **`<family>.gpu.nvidia-driver`** (6 aile, 6 kayıt): paket adı doğrulanmadı;
  `package = "TODO_VERIFY"`, `verification_status = "todo_verify"`, `risk = "high"`.
  Plandan `TodoVerify` nedeniyle çıkarılır ve hiçbir install komutu üretilmez. GPU
  profili seçildiğinde GUI'de "GPU / Sürücüler" kategorisinde yalnızca rehber olarak görünür.

## Kapsam dışı / ek doğrulama bekleyenler

- **Steam ve benzeri:** Arch'ta `multilib`, Debian'da `non-free`, Fedora/openSUSE'te
  ek repo gerektirdiği için v0.1 kapsamı dışıdır; paket adı yayınlanmaz.
- **vkbasalt:** Arch resmi depolarında yok (AUR); Fedora dist-git'te yok. Yalnızca
  Debian ve openSUSE için eklendi.
- **lutris:** Debian stable'da yok; Alpine'de yok. Diğer ailelerde var.
- **gamescope:** Debian stable'da yok; Alpine/Void'de var.
- **prismlauncher:** Debian stable ve Fedora'da yok; yalnızca Arch ve openSUSE.
- **ffmpeg (Fedora):** Fedora'da `ffmpeg` RPM Fusion gerektirir; `ffmpeg-free` farklı
  bir addır. Bu nedenle Fedora için eklenmedi.
- **thunderbird (openSUSE Tumbleweed oss):** oss deposunda yok (Mozilla deposu
  gerektirir); openSUSE için eklenmedi.
- **git-delta:** Alpine ve Void paket indekslerinde yok; bu iki aile için eklenmedi.
- **nmap:** openSUSE Tumbleweed ve Void'de doğrulanamadı; hiçbir aileye eklenmedi.
- **docker:** Fedora ana deposunda yok; konteyner için `podman` tercih edildi.
- **ardour:** Fedora'da yok.
- **distrobox:** Void'de yok (Alpine'de var).
- **obs-studio / mangohud:** Void'de yok.
- **clang:** Alpine paket indeksinde yok.
- **GPU sürücü paketleri (nvidia/amd/intel):** v0.1'de de yalnızca rehber; paket adı
  yayınlanmaz.
#!/usr/bin/env bash
# Publishes the current main branch and a version tag, then waits for the
# GitHub release that .github/workflows/release.yml builds.
#
# Everything that needs GitHub credentials happens through the user's own git
# remote, so this script never stores or prints a token.
#
# Usage:
#   ./scripts/deploy.sh              # push main + tag v<version>
#   ./scripts/deploy.sh --tag-only   # only push the tag
#   ./scripts/deploy.sh --dry-run    # checks only, changes nothing
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="full"
case "${1:-}" in
    --tag-only) MODE="tag" ;;
    --dry-run) MODE="dry" ;;
    "") ;;
    *)
        echo "Bilinmeyen argüman: $1" >&2
        echo "Kullanım: $0 [--tag-only|--dry-run]" >&2
        exit 2
        ;;
esac

fail() {
    echo "HATA: $*" >&2
    exit 1
}

step() {
    echo
    echo "==> $*"
}

# ---------------------------------------------------------------------------
# Preconditions
# ---------------------------------------------------------------------------

step "Ön kontroller"

git rev-parse --is-inside-work-tree >/dev/null 2>&1 || fail "Bu dizin bir git deposu değil."

if [ -n "$(git status --porcelain)" ]; then
    git status --short >&2
    fail "Çalışma ağacı temiz değil. Önce commit et veya stash'le."
fi
echo "  çalışma ağacı temiz"

if git ls-files --error-unmatch opencode.json >/dev/null 2>&1; then
    fail "opencode.json izleniyor. API anahtarı içeriyorsa commit'ten çıkar."
fi
echo "  opencode.json izlenmiyor"

REMOTE_URL="$(git remote get-url origin 2>/dev/null || true)"
[ -n "$REMOTE_URL" ] || fail "origin uzak deposu tanımlı değil."
echo "  origin: $REMOTE_URL"

SLUG="$(printf '%s' "$REMOTE_URL" \
    | sed -e 's#^git@github.com:#https://github.com/#' \
          -e 's#^ssh://git@github.com/##' \
          -e 's#^https://github.com/##' \
          -e 's#\.git$##')"

case "$SLUG" in
    */*) ;;
    *) fail "origin bir GitHub deposu gibi görünmüyor: $REMOTE_URL" ;;
esac

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' crates/setupmate-gui/Cargo.toml | head -n1)"
[ -n "$VERSION" ] || fail "Sürüm crates/setupmate-gui/Cargo.toml içinden okunamadı."
TAG="v$VERSION"
echo "  sürüm: $VERSION (etiket: $TAG)"

if ! command -v curl >/dev/null 2>&1; then
    fail "curl bulunamadı."
fi

API="https://api.github.com/repos/$SLUG"
CODE="$(curl -s -o /dev/null -w '%{http_code}' --max-time 20 "$API" || echo 000)"

case "$CODE" in
    200)
        echo "  depo mevcut: https://github.com/$SLUG"
        ;;
    404)
        cat >&2 <<EOF

HATA: https://github.com/$SLUG bulunamadı (HTTP 404).

Depoyu bir kez oluşturman gerekiyor:
  1. https://github.com/new adresini aç
  2. Repository name: SetupMate
  3. Public/Private seç, "Add a README" ve diğer başlangıç dosyalarını EKLEME
  4. Create repository

Sonra bu betiği tekrar çalıştır.
EOF
        exit 1
        ;;
    401|403)
        fail "GitHub API erişimi reddedildi (HTTP $CODE). Hız sınırı veya kimlik doğrulama sorunu."
        ;;
    000)
        fail "GitHub API'ye ulaşılamadı (ağ hatası)."
        ;;
    *)
        fail "GitHub API beklenmeyen yanıt döndü: HTTP $CODE"
        ;;
esac

if [ "$MODE" = "dry" ]; then
    step "Dry-run: değişiklik yapılmadı"
    echo "  Yapılacaklar: git push origin main; git tag $TAG; git push origin $TAG"
    exit 0
fi

# ---------------------------------------------------------------------------
# Push
# ---------------------------------------------------------------------------

if [ "$MODE" = "full" ]; then
    step "main dalı gönderiliyor"
    git push -u origin main
fi

step "Etiket hazırlanıyor: $TAG"
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    echo "  etiket zaten var"
else
    git tag -a "$TAG" -m "SetupMate $VERSION"
    echo "  etiket oluşturuldu"
fi

step "Etiket gönderiliyor"
git push origin "$TAG"

# ---------------------------------------------------------------------------
# Wait for the release workflow
# ---------------------------------------------------------------------------

step "Release workflow bekleniyor (en fazla 20 dakika)"
RELEASE_API="$API/releases/tags/$TAG"
DEADLINE=$(( $(date +%s) + 1200 ))
RELEASE_JSON=""

while [ "$(date +%s)" -lt "$DEADLINE" ]; do
    BODY="$(curl -s --max-time 20 -H 'Accept: application/vnd.github+json' "$RELEASE_API" || true)"
    if printf '%s' "$BODY" | grep -q '"assets"'; then
        RELEASE_JSON="$BODY"
        break
    fi
    printf '.'
    sleep 15
done
printf '\n'

if [ -z "$RELEASE_JSON" ]; then
    cat >&2 <<EOF

Etiket gönderildi ama release 20 dakika içinde görünmedi.
İlerlemeyi kontrol et:
  https://github.com/$SLUG/actions

Workflow başarılıysa doğrulama:
  ./SetupMate-$VERSION-x86_64.AppImage --check-update
EOF
    exit 1
fi

step "Release hazır"
printf '%s' "$RELEASE_JSON" \
    | tr ',' '\n' \
    | sed -n 's/.*"browser_download_url": *"\([^"]*\)".*/  \1/p'

cat <<EOF

Sonraki adımlar:
  cd dist
  curl -fsSLO https://github.com/$SLUG/releases/download/$TAG/SetupMate-$VERSION-x86_64.AppImage
  curl -fsSLO https://github.com/$SLUG/releases/download/$TAG/SHA256SUMS
  sha256sum -c SHA256SUMS
  ./SetupMate-$VERSION-x86_64.AppImage --check-update

AUR için (release tarball'ı artık mevcut):
  cd packaging/aur && updpkgsums && makepkg --printsrcinfo > .SRCINFO
EOF

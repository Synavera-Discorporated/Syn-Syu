pkgname=syn-syu
pkgver=0.1
pkgrel=1
pkgdesc="Syn-Syu — Synavera's conscious successor to pacman -Syu"
arch=('x86_64')
url="https://example.invalid/syn-syu"
license=('custom')
depends=('bash' 'jq' 'python' 'pacman' 'sudo')
makedepends=('rust' 'cargo' 'git')
provides=('syn-syu' 'synsyu')
conflicts=('syn-syu-git' 'synsyu-git')
source=("syn-syu::git+file://${PWD}")
sha256sums=('SKIP')

build() {
  cd "$srcdir/syn-syu/synsyu_core"
  cargo build --release
}

package() {
  cd "$srcdir/syn-syu"

  # Binaries
  install -Dm755 synsyu_core/target/release/synsyu_core "$pkgdir/usr/bin/synsyu_core"
  install -Dm755 synsyu/syn-syu "$pkgdir/usr/bin/syn-syu"

  # Shell library modules (Syn-Syu searches /usr/lib/syn-syu)
  install -Dm644 synsyu/lib/logging.sh  "$pkgdir/usr/lib/syn-syu/logging.sh"
  install -Dm644 synsyu/lib/helpers.sh  "$pkgdir/usr/lib/syn-syu/helpers.sh"
  install -Dm644 synsyu/lib/manifest.sh "$pkgdir/usr/lib/syn-syu/manifest.sh"

  # Docs and examples
  install -Dm644 docs/Syn-Syu_Overview.md "$pkgdir/usr/share/doc/syn-syu/Overview.md"
  install -Dm644 synsyu_core/README.md    "$pkgdir/usr/share/doc/syn-syu/Core_README.md"
  install -Dm644 examples/config.toml     "$pkgdir/usr/share/syn-syu/examples/config.toml"

  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}

# Install on Arch Linux / Omarchy

Ebbi is a native Linux application. This recipe builds the local **0.1.0** checkout;
it is not an AUR submission or a signed binary release. Build as your normal user.

## Requirements

Use an updated Arch/Omarchy system with `base-devel`, `cargo` (Rust 1.89 or newer),
`pkgconf` and `desktop-file-utils`. The package declares its runtime dependencies;
`makepkg` reports missing packages rather than installing them automatically.
A Wayland session and working Mesa/GPU drivers are needed for the graphical app.
The native file chooser also needs `xdg-desktop-portal` and an appropriate session
backend providing FileChooser. On Hyprland, `xdg-desktop-portal-gtk` supplies
[the default file chooser fallback](https://wiki.hypr.land/Hypr-Ecosystem/xdg-desktop-portal-hyprland/).

## Build a package from the checkout

Run these commands in Bash from the repository root. Fetching locked Rust
dependencies needs Internet the first time; the package build then runs offline.
The archive includes only source/build inputs and licenses, excluding Git metadata,
build output and personal databases. The temporary directory is printed for review.

```bash
cargo fetch --locked
build_dir=$(mktemp -d /tmp/ebbi-package.XXXXXXXX)
cp packaging/arch/PKGBUILD "$build_dir/PKGBUILD"
tar -czf "$build_dir/ebbi-0.1.0.tar.gz" \
    --transform='s,^,ebbi-0.1.0/,' \
    Cargo.toml Cargo.lock README.md src tests examples docs/screenshots \
    packaging/ebbi.desktop packaging/ebbi.png packaging/arch/PKGBUILD \
    docs/installation.md docs/usage.md docs/sample-data.md LICENSE-MIT LICENSE-APACHE
printf 'Package build directory: %s\n' "$build_dir"
cd "$build_dir"
makepkg --geninteg >> PKGBUILD
makepkg
```

`--geninteg` records the checksum of your local snapshot before building; it does
not authenticate the source checkout. `Cargo.lock` fixes dependency versions;
compiler/system changes can still change the resulting binary. Inspect the package:

```bash
pacman -Qip ./ebbi-0.1.0-2-x86_64.pkg.tar.zst
pacman -Qlp ./ebbi-0.1.0-2-x86_64.pkg.tar.zst
```

When ready, install it explicitly with:

```bash
sudo pacman -U ./ebbi-0.1.0-2-x86_64.pkg.tar.zst
```

The package owns `/usr/bin/ebbi`, `/usr/share/applications/ebbi.desktop` and its
license files and `/usr/share/icons/hicolor/512x512/apps/ebbi.png`. It adds **Ebbi** to the application launcher, with Study and
Analytics actions where the launcher supports desktop actions. The default entry
opens Dashboard. The icon is the owner's supplied bird image. The window's
application ID matches the desktop entry so Wayland can associate it with Ebbi.

## Launch and data

```bash
ebbi
ebbi --view=study
ebbi --view=analytics
ebbi --view=settings
# Isolated trial; never point --db at a legacy app's database:
ebbi --db /tmp/ebbi-trial/ebbi.db
```

Ebbi stores its database in `${XDG_DATA_HOME:-$HOME/.local/share}/ebbi/ebbi.db`.
Installation does not import or change Habitodo/Repasito data. Use Settings to
preview an explicit legacy import and make verified backups. A second launch
using the same database sends its requested view to the existing process and
exits. An open form or operation finishes before the view changes. Separate
`--db` workspaces can run independently. Wayland may require you to select the
existing window yourself. Closing the window exits the process.

Omarchy's Super+Space remains available for its menu.

## Remove or upgrade

Install a newly built package with `pacman -U` as above. To remove the application:

```bash
sudo pacman -R ebbi
```

Pacman removes packaged files; your database and backups remain in your data
directory. Back up your data through Settings before upgrading. A package rollback
does not roll back the database schema; an older binary may refuse a newer schema.

The recipe follows the [Arch Rust package guidelines](https://wiki.archlinux.org/title/Rust_package_guidelines).

#!/bin/sh
# Build and install cm-shim.
#
#   ./install.sh              build and install
#   ./install.sh uninstall    remove the binary and all launcher overrides
#
# Installs to ~/.local/bin. If that is not in your PATH, the script offers
# /usr/local/bin instead and uses sudo for that one copy step only.
#   PREFIX=/some/where ./install.sh     skips the question
set -eu

cd "$(dirname "$0")"

USER_BIN="$HOME/.local/bin"
SYSTEM_BIN="/usr/local/bin"
CONF_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/cm-shim"
APPS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
MARKER="# cm-shim override of "

in_path() {
    case ":$PATH:" in *":$1:"*) return 0 ;; *) return 1 ;; esac
}

# Prints the launcher overrides made by `cm-shim install`, one per line.
our_launchers() {
    for f in "$APPS_DIR"/*.desktop; do
        [ -f "$f" ] || continue
        if head -n 1 "$f" | grep -q "^$MARKER"; then
            echo "$f"
        fi
    done
}

choose_bin_dir() {
    if [ -n "${PREFIX:-}" ]; then
        echo "$PREFIX/bin"
    elif in_path "$USER_BIN" || [ ! -t 0 ]; then
        echo "$USER_BIN"
    else
        printf '%s is not in your PATH.\nInstall system-wide to %s instead? (needs sudo) [y/N] ' \
            "$USER_BIN" "$SYSTEM_BIN" >&2
        read -r answer
        case "$answer" in
            y | Y | yes | YES) echo "$SYSTEM_BIN" ;;
            *) echo "$USER_BIN" ;;
        esac
    fi
}

do_install() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "cargo not found. Install Rust first (Arch: pacman -S rust, or https://rustup.rs)." >&2
        exit 1
    fi

    # Always build as the normal user, never under sudo.
    echo "==> building"
    cargo build --release

    BIN_DIR="$(choose_bin_dir)"
    echo "==> installing $BIN_DIR/cm-shim"
    if [ -w "$BIN_DIR" ] || { [ ! -e "$BIN_DIR" ] && mkdir -p "$BIN_DIR" 2>/dev/null; }; then
        install -Dm755 target/release/cm-shim "$BIN_DIR/cm-shim"
    else
        sudo install -Dm755 target/release/cm-shim "$BIN_DIR/cm-shim"
    fi

    # A fully commented config, so defaults apply until the user edits it.
    if [ ! -e "$CONF_DIR/config" ]; then
        echo "==> writing sample config $CONF_DIR/config"
        mkdir -p "$CONF_DIR"
        cat > "$CONF_DIR/config" <<'CONF'
# cm-shim config. Flags on the command line override these.
#
# space: what the app renders to. Set the SAME space as your app's display
# profile, or the colors will be wrong with no warning.
#
# If this is left unset and no -s/-p/-t flag is given, the shim declares
# adobe_rgb and prints a line saying so.
#
#   adobe_rgb  rec2020_g22  display_p3
#   linear_rec709  linear_rec2020  pq_rec2020  hlg_rec2020  pq_p3  hlg_p3
#   display          bypass: the app uses the real display ICC
#space = adobe_rgb

# Manual alternative to `space` (both lines, protocol names, see wayland-info):
#primaries = display_p3
#tf = gamma22

# intent: perceptual | relative | relative_bpc | absolute | saturation
#intent = perceptual

# assume_kde_srgb_is_unmanaged: KDE only, default 1.
#
# KWin does not switch its color management off when the display profile is set
# to None. It keeps the protocol up, says the output is plain sRGB, and converts
# everything into that for a monitor it assumes is sRGB - so declaring a wide
# space into it makes your colors worse, not better. When the shim sees that
# answer it hands KWin's own description back instead, so nothing is converted.
# That is the same result Hyprland gives by switching its color management off.
#
# Set this to 0 if your monitor really is sRGB and you have a real sRGB profile
# loaded in KDE. There KWin says sRGB and means it, and declaring is correct.
#
# Ignored anywhere but KDE.
#assume_kde_srgb_is_unmanaged = 1
CONF
    fi

    if ! in_path "$BIN_DIR"; then
        echo "note: $BIN_DIR is not in your PATH; add it, or call $BIN_DIR/cm-shim directly."
    fi

    # Launchers carry the absolute path of the binary that created them.
    stale=""
    old_ifs=$IFS
    IFS='
'
    for f in $(our_launchers); do
        grep -q "^Exec=\"\{0,1\}$BIN_DIR/cm-shim" "$f" || stale="$stale\n    $f"
    done
    IFS=$old_ifs
    if [ -n "$stale" ]; then
        printf 'note: these launchers point at a different cm-shim location;\n      re-run "cm-shim install <app>" for them:%b\n' "$stale"
    fi

    echo "done. Try:  cm-shim -s adobe_rgb run gimp"
    echo "            cm-shim -s adobe_rgb install gimp"
}

do_uninstall() {
    # Launcher overrides point at the binary, so they must go with it,
    # otherwise those apps would no longer start from the menu.
    old_ifs=$IFS
    IFS='
'
    for f in $(our_launchers); do
        echo "==> removing launcher override $f"
        rm -- "$f"
    done
    IFS=$old_ifs

    for dir in "$USER_BIN" "$SYSTEM_BIN" ${PREFIX:+"$PREFIX/bin"}; do
        [ -e "$dir/cm-shim" ] || continue
        echo "==> removing $dir/cm-shim"
        if [ -w "$dir" ]; then
            rm -- "$dir/cm-shim"
        else
            sudo rm -- "$dir/cm-shim"
        fi
    done

    echo "done. Your config in $CONF_DIR was left in place."
}

case "${1:-install}" in
    install) do_install ;;
    uninstall) do_uninstall ;;
    *) echo "usage: $0 [install|uninstall]" >&2; exit 1 ;;
esac

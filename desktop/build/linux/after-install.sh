#!/bin/bash
# electron-builder expands dollar-brace macros in this file, so plain $VAR only.

APP_DIR=/opt/Coppice
EXECUTABLE=coppice
# Empty when installed. Tests point it at a scratch root.
ROOT=

if type update-alternatives >/dev/null 2>&1; then
    # Remove a previous link that does not use update-alternatives.
    if [ -L "/usr/bin/$EXECUTABLE" ] && [ -e "/usr/bin/$EXECUTABLE" ] \
        && [ "$(readlink "/usr/bin/$EXECUTABLE")" != "/etc/alternatives/$EXECUTABLE" ]; then
        rm -f "/usr/bin/$EXECUTABLE"
    fi
    update-alternatives --install "/usr/bin/$EXECUTABLE" "$EXECUTABLE" "$APP_DIR/$EXECUTABLE" 100 \
        || ln -sf "$APP_DIR/$EXECUTABLE" "/usr/bin/$EXECUTABLE"
else
    ln -sf "$APP_DIR/$EXECUTABLE" "/usr/bin/$EXECUTABLE"
fi

chown root:root "$APP_DIR/chrome-sandbox" || true
chmod 4755 "$APP_DIR/chrome-sandbox" || true

if hash update-mime-database 2>/dev/null; then
    update-mime-database /usr/share/mime || true
fi

if hash update-desktop-database 2>/dev/null; then
    update-desktop-database /usr/share/applications || true
fi

# First install registers the apt source, the way VS Code does. Later updates
# arrive through apt upgrade or Software Updater. The keyring is the committed
# public file packaged by scripts/dist.mjs, and the source list is written
# beside it. This script does not embed the repository URL.
# BEGIN apt source
KEY_SRC="$APP_DIR/resources/apt/coppice-archive-keyring.gpg"
LIST_SRC="$APP_DIR/resources/apt/coppice.list"
KEY_DST="$ROOT/usr/share/keyrings/coppice-archive-keyring.gpg"
LIST="$ROOT/etc/apt/sources.list.d/coppice.list"
# The copy Coppice last wrote. A source the user disabled or edited no longer
# matches it, so upgrades leave that file alone.
WRITTEN="$ROOT/var/lib/coppice-desktop/coppice.list.written"
coppice_owns_list() {
    [ ! -e "$LIST" ] && return 0
    if [ -f "$WRITTEN" ]; then
        cmp -s "$LIST" "$WRITTEN"
        return
    fi
    # Installed before the record existed: one active deb line on our keyring.
    [ "$(grep -cv '^[[:space:]]*$' "$LIST")" = 1 ] \
        && grep -q '^deb .*signed-by=/usr/share/keyrings/coppice-archive-keyring.gpg' "$LIST"
}
if [ -f "$KEY_SRC" ] && [ -f "$LIST_SRC" ]; then
    install -d -m 0755 "$ROOT/usr/share/keyrings"
    install -m 0644 "$KEY_SRC" "$KEY_DST"
    if coppice_owns_list; then
        install -d -m 0755 "$ROOT/etc/apt/sources.list.d" "$ROOT/var/lib/coppice-desktop"
        install -m 0644 "$LIST_SRC" "$LIST"
        install -m 0644 "$LIST_SRC" "$WRITTEN"
    fi
fi
# END apt source

# Ubuntu 24.04+ restricts unprivileged user namespaces; the profile grants them to
# Coppice. AppArmor 3 (Ubuntu 22.04) cannot parse abi/4.0 and needs no profile.
APPARMOR_SOURCE="$APP_DIR/resources/apparmor-profile"
APPARMOR_TARGET=/etc/apparmor.d/coppice
if [ -d /sys/kernel/security/apparmor ] && hash apparmor_parser 2>/dev/null; then
    if apparmor_parser --skip-kernel-load --debug "$APPARMOR_SOURCE" >/dev/null 2>&1; then
        cp -f "$APPARMOR_SOURCE" "$APPARMOR_TARGET"
        if ! { [ -x /usr/bin/ischroot ] && /usr/bin/ischroot; }; then
            apparmor_parser -r --write-cache --skip-read-cache "$APPARMOR_TARGET" || true
        fi
    else
        echo "Skipping the Coppice AppArmor profile: this AppArmor version does not support it"
    fi
fi

exit 0

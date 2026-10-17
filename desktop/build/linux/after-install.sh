#!/bin/bash
# electron-builder expands dollar-brace macros in this file, so plain $VAR only.

APP_DIR=/opt/Coppice
EXECUTABLE=coppice

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
# arrive through apt upgrade or Software Updater. The key is packaged only
# when the release build has COPPICE_APT_SIGNING_KEY; local builds omit it.
KEY_SRC="$APP_DIR/resources/apt/coppice-archive-keyring.gpg"
KEY_DST=/usr/share/keyrings/coppice-archive-keyring.gpg
LIST=/etc/apt/sources.list.d/coppice.list
if [ -f "$KEY_SRC" ]; then
    install -d -m 0755 /usr/share/keyrings
    install -m 0644 "$KEY_SRC" "$KEY_DST"
    cat > "$LIST" << 'EOF'
deb [arch=amd64,arm64 signed-by=/usr/share/keyrings/coppice-archive-keyring.gpg] https://fertile-org.github.io/coppice stable main
EOF
    chmod 644 "$LIST"
fi

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

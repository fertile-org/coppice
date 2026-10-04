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

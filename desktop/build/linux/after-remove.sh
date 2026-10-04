#!/bin/bash
# electron-builder expands dollar-brace macros in this file, so plain $VAR only.

APP_DIR=/opt/Coppice
EXECUTABLE=coppice
APPARMOR_TARGET=/etc/apparmor.d/coppice

if type update-alternatives >/dev/null 2>&1; then
    update-alternatives --remove "$EXECUTABLE" "$APP_DIR/$EXECUTABLE" || true
else
    rm -f "/usr/bin/$EXECUTABLE"
fi

if [ -f "$APPARMOR_TARGET" ]; then
    if [ -d /sys/kernel/security/apparmor ] && hash apparmor_parser 2>/dev/null \
        && ! { [ -x /usr/bin/ischroot ] && /usr/bin/ischroot; }; then
        apparmor_parser -R "$APPARMOR_TARGET" || true
    fi
    rm -f "$APPARMOR_TARGET"
fi

exit 0

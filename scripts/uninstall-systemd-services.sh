#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
    echo "This script must be run as root (try: sudo bash scripts/uninstall-systemd-services.sh)"
    exit 1
fi

for cmd in rm systemctl; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Missing required command: $cmd"
        exit 1
    fi
done

echo "==> Disabling and stopping monsgeek services"
systemctl disable --now monsgeek-inputd.service monsgeek-driver.service || true

echo "==> Removing unit files from /etc/systemd/system"
rm -f /etc/systemd/system/monsgeek-driver.service
rm -f /etc/systemd/system/monsgeek-inputd.service

echo "==> Removing installed device registry files"
rm -rf /usr/share/monsgeek/protocol/devices
rmdir /usr/share/monsgeek/protocol 2>/dev/null || true
rmdir /usr/share/monsgeek 2>/dev/null || true

echo "==> Reloading systemd daemon"
systemctl daemon-reload

#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
    echo "This script must be run as root (try: sudo bash scripts/install-systemd-services.sh)"
    exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SYSTEMD_DIR="/etc/systemd/system"
REGISTRY_INSTALL_DIR="/usr/share/monsgeek/protocol/devices"
CONFIG_INSTALL_DIR="/etc/monsgeek"

for cmd in cp systemctl; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Missing required command: $cmd"
        exit 1
    fi
done

echo "==> Installing monsgeek systemd unit files"
cp "$ROOT_DIR/deploy/systemd/monsgeek-driver.service" "$SYSTEMD_DIR/monsgeek-driver.service"
cp "$ROOT_DIR/deploy/systemd/monsgeek-inputd.service" "$SYSTEMD_DIR/monsgeek-inputd.service"

echo "==> Installing device registry files"
mkdir -p "$REGISTRY_INSTALL_DIR"
cp -a "$ROOT_DIR/crates/monsgeek-protocol/devices/." "$REGISTRY_INSTALL_DIR/"

echo "==> Installing transport runtime config"
mkdir -p "$CONFIG_INSTALL_DIR"
cp "$ROOT_DIR/deploy/config/transport-config.json" "$CONFIG_INSTALL_DIR/transport-config.json"

echo "==> Reloading systemd daemon"
systemctl daemon-reload

echo "==> Enabling and starting monsgeek services"
systemctl enable --now monsgeek-driver.service monsgeek-inputd.service

if systemctl list-unit-files | awk '{print $1}' | grep -Fxq "monsgeek-hid.service"; then
    echo "==> Disabling legacy service: monsgeek-hid.service"
    systemctl disable --now monsgeek-hid.service || true
fi

echo "==> Verifying service states"
for svc in monsgeek-driver.service monsgeek-inputd.service; do
    enabled="$(systemctl is-enabled "$svc")"
    active="$(systemctl is-active "$svc")"
    echo "$svc enabled=$enabled active=$active"
done

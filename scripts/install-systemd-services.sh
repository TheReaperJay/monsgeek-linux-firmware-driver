#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
    echo "This script must be run as root (try: sudo bash scripts/install-systemd-services.sh)"
    exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SYSTEMD_DIR="/etc/systemd/system"
BINARY_INSTALL_DIR="/usr/bin"
REGISTRY_INSTALL_DIR="/usr/share/monsgeek/protocol/devices"
CONFIG_INSTALL_DIR="/etc/monsgeek"
MODPROBE_DIR="/etc/modprobe.d"

for cmd in cargo cp install systemctl; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Missing required command: $cmd"
        exit 1
    fi
done

echo "==> Building release binaries"
(
    cd "$ROOT_DIR"
    cargo build --release -p monsgeek-driver -p monsgeek-inputd -p monsgeek-cli
)

echo "==> Installing binaries to $BINARY_INSTALL_DIR"
install -Dm755 "$ROOT_DIR/target/release/monsgeek-driver" \
    "$BINARY_INSTALL_DIR/monsgeek-driver"
install -Dm755 "$ROOT_DIR/target/release/monsgeek-inputd" \
    "$BINARY_INSTALL_DIR/monsgeek-inputd"
install -Dm755 "$ROOT_DIR/target/release/monsgeek-cli" \
    "$BINARY_INSTALL_DIR/monsgeek-cli"

echo "==> Installing monsgeek systemd unit files"
cp "$ROOT_DIR/deploy/systemd/monsgeek-driver.service" "$SYSTEMD_DIR/monsgeek-driver.service"
cp "$ROOT_DIR/deploy/systemd/monsgeek-inputd.service" "$SYSTEMD_DIR/monsgeek-inputd.service"

echo "==> Installing device registry files"
mkdir -p "$REGISTRY_INSTALL_DIR"
cp -a "$ROOT_DIR/crates/monsgeek-protocol/devices/." "$REGISTRY_INSTALL_DIR/"

echo "==> Installing transport runtime config"
mkdir -p "$CONFIG_INSTALL_DIR"
cp "$ROOT_DIR/deploy/config/transport-config.json" "$CONFIG_INSTALL_DIR/transport-config.json"

echo "==> Installing usbhid quirk config"
mkdir -p "$MODPROBE_DIR"
cp "$ROOT_DIR/crates/monsgeek-transport/deploy/monsgeek-hid-usbhid.conf" \
   "$MODPROBE_DIR/monsgeek-hid-usbhid.conf"

echo "==> Reloading systemd daemon"
systemctl daemon-reload

echo "==> Enabling and restarting monsgeek services"
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

echo "NOTE: usbhid quirks apply after reboot (or after reloading the built-in usbhid stack)."

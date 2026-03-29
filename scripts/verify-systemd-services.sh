#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SELF_PATH="$ROOT_DIR/scripts/verify-systemd-services.sh"
POSTBOOT_SERVICE="monsgeek-postreboot-verify.service"
POSTBOOT_LOG="/var/log/monsgeek-postreboot-verify.log"
WITH_REBOOT=0
POST_REBOOT=0

usage() {
    cat <<'EOF'
Usage:
  sudo bash scripts/verify-systemd-services.sh [--with-reboot]
  sudo bash scripts/verify-systemd-services.sh --post-reboot

Modes:
  default         Run install + active/enabled checks + CLI checks + crash-restart checks.
  --with-reboot   Also schedule automatic post-reboot checks and reboot host.
  --post-reboot   Internal mode used by systemd service after reboot.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --with-reboot)
            WITH_REBOOT=1
            ;;
        --post-reboot)
            POST_REBOOT=1
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
    shift
done

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
    echo "Run as root: sudo bash scripts/verify-systemd-services.sh [--with-reboot]"
    exit 1
fi

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1"
        exit 1
    fi
}

for cmd in systemctl bash sleep pkill tee; do
    need_cmd "$cmd"
done

log() {
    echo "[verify-systemd] $*"
}

run_check() {
    log "$*"
    "$@"
}

wait_for_service_active() {
    local service="$1"
    local timeout_sec="${2:-30}"
    local elapsed=0

    while (( elapsed < timeout_sec )); do
        if systemctl is-active --quiet "$service"; then
            log "$service is active"
            return 0
        fi
        local state
        state="$(systemctl is-active "$service" 2>/dev/null || true)"
        log "waiting for $service (current state: ${state:-unknown})"
        sleep 1
        ((elapsed += 1))
    done

    echo "Service did not become active within ${timeout_sec}s: $service"
    systemctl status "$service" --no-pager || true
    journalctl -u "$service" -n 50 --no-pager || true
    exit 1
}

check_enabled_active() {
    run_check systemctl is-enabled monsgeek-driver.service monsgeek-inputd.service
    wait_for_service_active monsgeek-driver.service 30
    wait_for_service_active monsgeek-inputd.service 30
}

check_cli() {
    if command -v monsgeek-cli >/dev/null 2>&1; then
        run_check monsgeek-cli devices list
        run_check monsgeek-cli info
        return
    fi

    log "`monsgeek-cli` not found on PATH; falling back to `cargo run -p monsgeek-cli`"
    need_cmd cargo

    if [[ -n "${SUDO_USER:-}" ]] && [[ "${SUDO_USER}" != "root" ]] && command -v runuser >/dev/null 2>&1; then
        run_check runuser -u "$SUDO_USER" -- bash -lc "cd '$ROOT_DIR' && cargo run -q -p monsgeek-cli -- devices list"
        run_check runuser -u "$SUDO_USER" -- bash -lc "cd '$ROOT_DIR' && cargo run -q -p monsgeek-cli -- info"
    else
        run_check bash -lc "cd '$ROOT_DIR' && cargo run -q -p monsgeek-cli -- devices list"
        run_check bash -lc "cd '$ROOT_DIR' && cargo run -q -p monsgeek-cli -- info"
    fi
}

check_restart_policy() {
    local before after
    before="$(systemctl show -p NRestarts --value monsgeek-driver.service)"
    log "Driver restarts before crash test: $before"

    run_check pkill -9 monsgeek-driver
    wait_for_service_active monsgeek-driver.service 30

    after="$(systemctl show -p NRestarts --value monsgeek-driver.service)"
    log "Driver restarts after crash test: $after"

    if [[ "$after" =~ ^[0-9]+$ ]] && [[ "$before" =~ ^[0-9]+$ ]]; then
        if (( after <= before )); then
            echo "Restart counter did not increment (before=$before after=$after)"
            exit 1
        fi
    fi
}

install_postreboot_service() {
    cat >/etc/systemd/system/"$POSTBOOT_SERVICE" <<EOF
[Unit]
Description=MonsGeek post-reboot verification
After=multi-user.target
Wants=multi-user.target

[Service]
Type=oneshot
ExecStart=/bin/bash $SELF_PATH --post-reboot

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    systemctl enable "$POSTBOOT_SERVICE"
}

cleanup_postreboot_service() {
    systemctl disable "$POSTBOOT_SERVICE" >/dev/null 2>&1 || true
    rm -f /etc/systemd/system/"$POSTBOOT_SERVICE"
    systemctl daemon-reload
}

run_post_reboot_mode() {
    {
        log "Post-reboot verification started"
        cd "$ROOT_DIR"
        check_enabled_active
        log "Post-reboot verification passed"
    } | tee -a "$POSTBOOT_LOG"

    cleanup_postreboot_service
}

if [[ "$POST_REBOOT" == "1" ]]; then
    run_post_reboot_mode
    exit 0
fi

cd "$ROOT_DIR"
run_check bash scripts/install-systemd-services.sh
check_enabled_active
check_cli
check_restart_policy

if [[ "$WITH_REBOOT" == "1" ]]; then
    log "Scheduling automatic post-reboot verification"
    install_postreboot_service
    log "Rebooting now; post-reboot result will be written to $POSTBOOT_LOG"
    reboot
    exit 0
fi

log "Verification passed (without reboot check)."
log "Run with --with-reboot for one-command full reboot validation."

# MonsGeek Systemd Service Runbook

## Install

```bash
cd /home/whitebehemoth/Dev/Projects/monsgeek-firmware-driver
sudo bash scripts/install-systemd-services.sh
```

## One-Command Verification

Run the full install + active/enabled checks + CLI smoke + crash-restart check:

```bash
cd /home/whitebehemoth/Dev/Projects/monsgeek-firmware-driver
sudo bash scripts/verify-systemd-services.sh
```

Run everything above plus automatic reboot verification in one command:

```bash
cd /home/whitebehemoth/Dev/Projects/monsgeek-firmware-driver
sudo bash scripts/verify-systemd-services.sh --with-reboot
```

After reboot, the post-boot result is written to:

```bash
/var/log/monsgeek-postreboot-verify.log
```

## Status

```bash
systemctl is-enabled monsgeek-driver.service monsgeek-inputd.service
systemctl is-active monsgeek-driver.service monsgeek-inputd.service
systemctl status monsgeek-driver.service monsgeek-inputd.service
```

## Restart

```bash
sudo systemctl restart monsgeek-driver.service monsgeek-inputd.service
systemctl is-active monsgeek-driver.service monsgeek-inputd.service
```

## Log Tail

```bash
journalctl -u monsgeek-driver.service -f
journalctl -u monsgeek-inputd.service -f
```

## Crash-Restart Verification

```bash
sudo pkill -9 monsgeek-driver
sleep 2
systemctl status monsgeek-driver.service
systemctl is-active monsgeek-driver.service
systemctl show -p NRestarts monsgeek-driver.service
```

Expected result: `monsgeek-driver.service` returns to `active` and `NRestarts` increments.

## Reboot Verification

After host reboot:

```bash
systemctl is-enabled monsgeek-driver.service monsgeek-inputd.service
systemctl is-active monsgeek-driver.service monsgeek-inputd.service
```

Expected result: both services are still `enabled` and `active`.

## Uninstall

```bash
cd /home/whitebehemoth/Dev/Projects/monsgeek-firmware-driver
sudo bash scripts/uninstall-systemd-services.sh
```

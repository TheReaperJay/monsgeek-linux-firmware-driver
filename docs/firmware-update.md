# Firmware Update Runbook (M5W)

## Safety Model

Firmware update is high risk and can leave the device in bootloader mode if interrupted. The bridge OTA RPC is disabled by default and only available when `monsgeek-driver` is started with:

```bash
monsgeek-driver --enable-ota
```

## Preconditions

- Internet access for vendor auto-download, or a known-good local firmware file
- Physical recovery path available (DFU/recovery workflow)
- `monsgeek-driver` and `monsgeek-cli` installed
- Single target keyboard selected (avoid ambiguous multi-device sessions)

## Guarded CLI Policy

### Preferred Source (Default)

`firmware validate` and `firmware flash` default to official vendor auto-download (Electron-style API):

- query current device version via `GET_USB_VERSION`
- query vendor metadata by `device_id`
- download firmware binary to `./firmware/downloads` (or `--download-dir`)
- compare current USB firmware vs target USB firmware before flash approval

If a local file is used (`--file`), it is treated as unofficial and requires `--allow-unofficial`.

### Interactive

`firmware flash` requires an exact typed phrase:

- `FLASH M5W`

Any mismatch must fail before bootloader transfer starts.

### Non-Interactive

Non-interactive flashing must require **both** flags:

- `--yes`
- `--i-understand-firmware-risk`

`--yes` alone is insufficient and must fail.

## Expected Progress Phases

Bridge OTA progress stream should emit phase-oriented updates in this order:

1. `phase=preflight`
2. `phase=enter_bootloader`
3. `phase=wait_bootloader`
4. `phase=transfer_start`
5. `phase=transfer_chunks`
6. `phase=transfer_complete`
7. `phase=post_verify`
8. terminal success (`err` empty)

## Guarded Hardware Checkpoint

Use the helper script:

```bash
scripts/firmware-flash-checkpoint.sh
```

Optional local override:

```bash
scripts/firmware-flash-checkpoint.sh --firmware /absolute/path/to/m5w.bin
```

The script captures session logs and metadata, including endpoint details and firmware source.

## Failure Modes and Recovery Guidance

On integrity mismatch or incomplete transfer, responses must include all of:

- `device may still be in bootloader mode`
- `re-run with a known-good image`
- `use physical recovery path if device no longer enumerates`

Bootloader wait timeout policy is bounded:

- one controlled retry
- second timeout is hard failure

## Post-Flash Success Contract

Success is only valid after post-verify checks:

1. normal-mode re-enumeration
2. `GET_USB_VERSION` query
3. `GET_REV` when available

## Explicit Data-Risk Warning

Firmware update can erase application configuration region state. Always capture backup context before flashing and record backup path in session metadata.

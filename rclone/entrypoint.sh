#!/usr/bin/env sh
set -euo pipefail

# ----- Configuration from ENV -----
: "${RCLONE_REMOTE:?Need RCLONE_REMOTE e.g. google_drive:}"
: "${RCLONE_TARGET:?Need RCLONE_TARGET e.g. /mnt-rclone/google_drive}"
: "${RCLONE_CONFIG:=/config/rclone/rclone.conf}"
: "${VFS_CACHE_DIR:=/config/rclone/vfs-cache}"
: "${CHECK_INTERVAL:=10}"

# Build our base rclone command
RCLONE_CMD="rclone \
  --config=${RCLONE_CONFIG} \
  --log-level=INFO \
  --log-file=- \
  mount ${RCLONE_REMOTE} ${RCLONE_TARGET} \
    --allow-other \
    --allow-non-empty \
    --vfs-cache-mode writes \
    --cache-dir ${VFS_CACHE_DIR}"

# Function: try to mount and verify
do_mount() {
  echo "$(date '+%Y-%m-%d %H:%M:%S')  [INFO] Starting mount..."
  # exec so this rclone becomes PID 1
  exec sh -c "$RCLONE_CMD"
}

# Background watcher: if mount dies or disappears, restart the container
watch_mount() {
  while sleep "${CHECK_INTERVAL}"; do
    if ! mountpoint -q "${RCLONE_TARGET}"; then
      echo "$(date '+%Y-%m-%d %H:%M:%S')  [WARN] Mount lost, exiting so Docker restarts me"
      # If we exit non-zero, Docker will restart the whole container
      exit 1
    fi
  done
}

# Kick off the watcher in the background
watch_mount &

# Now do the mount (this `exec` never returns unless it fails)
do_mount

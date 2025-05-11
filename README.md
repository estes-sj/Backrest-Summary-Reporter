### Expected Summary Successful Payload Example
```json
{
  "task": "backup for plan \"local-sam-fedserver01-opt\"",
  "time": "2025-04-30T16:29:04-04:00",
  "event": "snapshot success",
  "repo": "sam-backupdrive01",
  "plan": "local-sam-fedserver01-opt",
  "snapshot": "2a74edafc0804df6d41fa4a79d395da84b62e394748bb42298bfcb34d53064c1",
  "snapshot_stats": {
    "message_type": "summary",
    "error": null,
    "during": "",
    "item": "",
    "files_new": 0,
    "files_changed": 0,
    "files_unmodified": 207,
    "dirs_new": 0,
    "dirs_changed": 0,
    "dirs_unmodified": 137,
    "data_blobs": 0,
    "tree_blobs": 0,
    "data_added": 0,
    "total_files_processed": 207,
    "total_bytes_processed": 4064437,
    "total_duration": 0.736078876,
    "snapshot_id": "2a74edafc0804df6d41fa4a79d395da84b62e394748bb42298bfcb34d53064c1",
    "percent_done": 0,
    "total_files": 0,
    "files_done": 0,
    "total_bytes": 0,
    "bytes_done": 0,
    "current_files": null
  }
}
```

which is a result of:

```bash
curl -X POST https://backrest-listener.teetunk.dev/summary \
     -H "Content-Type: application/json" \
     -H "X-API-Key: q2134gfq45gh34ygaqw4ertgsaawesrg" \
     --data-binary @- <<EOF
{
  "task": {{ .JsonMarshal .Task }},
  "time": "{{ .FormatTime .CurTime }}",
  "event": "{{ .EventName .Event }}",
  "repo": {{ .JsonMarshal .Repo.Id }},
  "plan": {{ .JsonMarshal .Plan.Id }},
  "snapshot": {{ .JsonMarshal .SnapshotId }}{{ if .Error }},
  "error": {{ .JsonMarshal .Error }}{{ else if .SnapshotStats }},
  "snapshot_stats": {{ .JsonMarshal .SnapshotStats }}{{ end }}
}
EOF
```

### Truncate all tables
```sql
TRUNCATE TABLE public.summaries, public.snapshot_stats RESTART IDENTITY CASCADE;
```

### Test Methods

#### Success Snapshot
```
curl -X POST https://backrest-listener.teetunk.dev/summary \
     -H "Content-Type: application/json" \
     -H "X-API-Key: q2134gfq45gh34ygaqw4ertgsaawesrg" \
     -d @tests/example.json
```

#### Bad API Key
curl -X POST https://backrest-listener.teetunk.dev/summary \
     -H "Content-Type: application/json" \
     -H "X-API-Key: HELLO WORLD" \
     -d @tests/example.json

#### Error
curl -X POST https://backrest-listener.teetunk.dev/summary \
     -H "Content-Type: application/json" \
     -H "X-API-Key: q2134gfq45gh34ygaqw4ertgsaawesrg" \
     -d @tests/error.json

## Setup on Backrest

- Recommend using in `Repository` settings instead of `Plan` settings (or whichever is less)

### Backrest Events

#### Snapshot start/end
These operations appear to not have a snapshot ID associated with them

## Setting up Storage Mounts
Storage mounts are mounted to the main API service to track and send storage statistics. Currently, this has been tested to work for local drives, local network drives via SSHFS, and rclone via FUSE.

Once exposed to the API container, storage paths can be tracked and recorded by adding them to the `.env` and/or `docker-compose.yaml`

<p align="center">
    <img src="docs/img/storage_stats_email.png" alt="Storage Stats" width="60%"/>
</p>

### Local Drives

Simple local drives include the drive the container is running on and any physically connected devices to the host machine (e.g. external HDD).

For these, the folder or any of its parents must be mounted in the volumes section of the `docker-compose.yaml`. In the below example, the host `/opt` is being mounted to `/mnt/opt` inside the container as `read-only`.

```yaml
# docker-compose.yaml

    volumes:
      # Host bind-mounts for local or remote mount points (read-only since we are only pulling stats)
      - /opt:/mnt/opt:ro
```

Now in the `.env`, we can specify the path to the volume inside the container (or any of its children) that we want to track. Additionally, we can specify an optional nickname that is used in the emails. If no nickname is used, the path will be used.

```conf
# .env

STORAGE_PATH_1=/mnt/opt
STORAGE_NICK_1=sam-fedserver01-opt
```

#### External Drives (Linux)

Externally connected drives, such as a USB external HDD, are recommended to be mounted via `fstab` to ensure they are available on system boot.

One way to do this is:

1. Connect the drive to the host machine.
2. Run `lsblk -f` to list the names, fs types, UUID, and mount points.
3. Find the drive you want to connect and copy its UUID, where you want to mount it to and the fs type.
4. Edit `/etc/fstab` to include a line for your new entry. An example of mounting to `/mnt`:
   ```
   UUID=f6b99246-8780-e989-9bb6-94211a0f0f50  /mnt  ext4  defaults  0  2
   ```
5. Save the file and apply with `mount -a`.

### Local Network Drives

Let's say there's a drive on another machine that is on the same local network as the host machine. One way of tracking it is through `SSHFS`. First, we need to make sure we have this installed.

**Fedora**
```bash
sudo dnf -y install sshfs
```

**Ubuntu**
```bash
sudo apt-get install sshfs
```

Then, we can similarly auto-mount via `fstab` as we did with locally connected drives.
1. Connect the drive to the machine on the local network and follow the steps on that machine for [external drives Linux](#external-drives-linux) if it needs to be added to that `fstab`.
2. Get the local ip address of the machine with `ifconfig`.
3. On the host machine, generate an SSH key if you don't already have one.
   ```bash
   ssh-keygen -t rsa -b 4096 -C "your_email@example.com"
   ```
4. Press Enter to accept default file location (`~/.ssh/id_rsa`)
5. Copy the Public Key to Machine 2:
   ```bash
   ssh-copy-id user@machine2_ip_or_hostname
   ```
   Replace user with your actual username on Machine 2 and enter the password one last time when prompted.
6. Test the connection:
   ```bash
   ssh user@machine2_ip_or_hostname
   ```
7. Edit `/etc/fstab` to include a line for your new entry for mounting via `SSHFS` and your working SSH key. An example of mounting to `/mnt/immich_remote` for the user `user`, ip `192.168.10.44`, and remote's `/mnt/.immich`:
   ```
   user@192.168.10.44:/mnt/.immich /mnt/immich_remote fuse.sshfs ro,allow_other,_netdev,IdentityFile=/root/.ssh/id_rsa,users,idmap=user,follow_symlinks 0 0
   ```
8. Save the file and apply with `mount -a`.

### Rclone Mounts

Rclone mounts, such as Google Drive, are supported by an `rclone-mounter` container that uses FUSE connections to mount the cloud connection to a shared directory across the rclone container, the host machine, and the API container. To make sure the FUSE mount is properly created and mounted, a healthcheck is used to prevent starting the API container until ready.

See a full example `docker-compose.yaml` for rclone mounts [TODO: here)]().

The below example shows the rclone `docker-compose.yaml` service that uses a pre-configured Google Drive mount `google_drive`. The pre-configured `rclone.conf` is stored in `./rclone/config` on the host machine.

❗❗ Note: For these FUSE mounts to work, the folder to mount to on the host machine must exist. In the example configuration, you would need to run `sudo mkdir -p /mnt-rclone/google_drive` before running the first time (if it doesn't already exist). ❗❗

To create an rclone config, see the [official rclone docs](https://rclone.org/commands/rclone_config/) for more info.

```yaml
  # Rclone container that handles mounting Google Drive via FUSE
  rclone-mounter:
    image: rclone/rclone:latest
    container_name: rclone-mounter
    restart: unless-stopped
    cap_add:
      - SYS_ADMIN               # Required for FUSE
    devices:
      - /dev/fuse               # Expose FUSE device
    security_opt:
      - apparmor:unconfined     # Unconfine AppArmor to allow FUSE mount
    volumes:
      # Config volume for rclone.conf
      - type: bind
        source: ./rclone/config
        target: /config/rclone

      # Optional: cache directory for VFS (improves stability/performance)
      - type: bind
        source: ./rclone/vfs-cache
        target: /config/rclone/vfs-cache

      # Mountpoint shared with host and other containers (Google Drive)
      # Ensure the folder exists on the host machine before running
      # e.g. sudo mkdir -p /mnt-rclone/google_drive
      - type: bind
        source: /path/to/desired/host/location # Mounted to the API container
        target: /mnt-rclone/google_drive
        bind:
          propagation: shared   # Allow mount propagation between containers
    command: >
      mount google_drive: /mnt-rclone/google_drive
      --config=/config/rclone/rclone.conf
      --allow-other
      --allow-non-empty
      --vfs-cache-mode writes
      --cache-dir /config/rclone/vfs-cache
    healthcheck:
      test: ["CMD-SHELL", "grep -q ' /mnt-rclone/google_drive ' /proc/mounts"]
      interval: 5s
      timeout: 2s
      retries: 5
      start_period: 5s
```
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
  "snapshot": {{ .JsonMarshal .SnapshotId }},
  {{- if .Error }}
  "error": {{ .JsonMarshal .Error }}
  {{- else if .SnapshotStats }}
  "snapshot_stats": {{ .JsonMarshal .SnapshotStats }}
  {{- end }}
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
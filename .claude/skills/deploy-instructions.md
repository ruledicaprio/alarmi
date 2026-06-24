# Rocky 9 Deployment Instructions

## Always
1. Download: `curl -O http://<internal-server>/alarmi/deployment.tar.gz`
2. Verify: `sha256sum deployment.tar.gz` (compare with checksum provided)
3. Unpack: `python3 unpack.py deployment.tar.gz <case>`

## Cases
### frontend
- Unpack script copies `dist/` to `/opt/alarmi/web/`, sets permissions.
- No service restart needed (static files).

### binary
- Unpack script stops service, replaces `/opt/alarmi/bin/alarmi-server`, starts service.
- It also runs `systemctl status alarmi-server` to confirm.

### schema
- Unpack script runs `psql -U alarmi -d alarmi_db -f <migration.sql>` for each SQL file in order.
- It uses `ON_ERROR_STOP=1` and rolls back on failure.
- It prints "Schema updated successfully" or the exact error.

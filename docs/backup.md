# Backup, restore and export

| What | Command | Notes |
| --- | --- | --- |
| Online backup | `tendly backup --out /path/tendly-2026-10-03.db` | Consistent snapshot (`VACUUM INTO`) while the server runs; refuses to overwrite |
| Restore | stop the server, then `tendly restore --from backup.db --yes` | Checks integrity; keeps the old database as `*.db.before-restore` |
| JSON export | `tendly export --out export.json`, or Settings → "Export everything" on the host | Excludes secrets: no tokens, keys, subscription URLs or share-link hashes |
| Calendar export | Calendar → Export (`.ics` or JSON) | Uses current filters |

Back up the encryption key separately and securely. Without it, stored provider tokens and subscription URLs in a backup can't be decrypted. You'd have to reconnect those, but everything else stays readable.

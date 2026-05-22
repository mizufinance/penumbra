# Compliance Release Notes

## View Database Reset Required For Slot Counts

Compliance slot support changes the view database schema. The
`compliance_asset_leaves` table now includes:

```sql
slot_count BIGINT NOT NULL
```

This is not migration-compatible with existing view databases. Operators should
back up any local data they need before resetting the view database, then
resynchronize with:

```bash
pcli view reset
```

`crates/view/src/storage.rs` computes `SCHEMA_HASH` from
`crates/view/src/storage/schema.sql` and checks it when opening an existing
database. Because this schema change alters that hash, old databases will be
rejected with the existing reset-and-resync error instead of being opened with a
partial schema.

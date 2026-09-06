-- Pinned native backend's structural contract. Existing user sessions are
-- never dropped or rewritten to make an incompatible deployment look valid.
WITH relation AS (
  SELECT oid FROM pg_catalog.pg_class
  WHERE oid=pg_catalog.to_regclass('tower_sessions.session') AND relkind='r'
), columns AS (
  SELECT a.attnum, a.attname::text AS name, a.atttypid, a.attnotnull,
         a.attgenerated, a.attidentity
  FROM pg_catalog.pg_attribute a JOIN relation r ON a.attrelid=r.oid
  WHERE a.attnum>0 AND NOT a.attisdropped
)
SELECT (SELECT count(*) FROM relation)=1
  AND (SELECT count(*) FROM columns)=3
  AND NOT EXISTS (
    SELECT 1 FROM columns
    WHERE NOT attnotnull OR attgenerated<>'' OR attidentity<>''
       OR NOT ((name='id' AND atttypid='pg_catalog.text'::regtype)
            OR (name='data' AND atttypid='pg_catalog.bytea'::regtype)
            OR (name='expiry_date' AND atttypid='pg_catalog.timestamptz'::regtype))
  )
  AND EXISTS (
    SELECT 1 FROM pg_catalog.pg_index i JOIN relation r ON r.oid=i.indrelid
    JOIN columns c ON c.name='id'
    WHERE i.indisprimary AND i.indisunique AND i.indisvalid AND i.indisready
      AND i.indimmediate AND i.indnkeyatts=1 AND i.indkey[0]=c.attnum
  );

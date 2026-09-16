-- Contract of pinned tower-sessions-sqlx-store 0.15.0, including native
-- persistence and write semantics. Never change user data to pass validation.
WITH relation AS (
  SELECT c.oid FROM pg_catalog.pg_class c
  JOIN pg_catalog.pg_am am ON am.oid=c.relam
  WHERE c.oid=pg_catalog.to_regclass('tower_sessions.session')
    AND c.relkind='r' AND c.relpersistence='p' AND am.amname='heap'
    AND NOT c.relrowsecurity AND NOT c.relforcerowsecurity
    AND NOT c.relispartition AND c.reloftype=0
), columns AS (
  SELECT a.attnum, a.attname::text AS name, a.atttypid, a.attnotnull,
         a.attgenerated, a.attidentity, a.atthasdef, a.atttypmod,
         a.attcollation, t.typcollation
  FROM pg_catalog.pg_attribute a JOIN relation r ON a.attrelid=r.oid
  JOIN pg_catalog.pg_type t ON t.oid=a.atttypid
  WHERE a.attnum>0 AND NOT a.attisdropped
)
SELECT (SELECT count(*) FROM relation)=1
  AND (SELECT count(*) FROM columns)=3
  AND NOT EXISTS (
    SELECT 1 FROM columns
    WHERE NOT attnotnull OR attgenerated<>'' OR attidentity<>''
       OR atthasdef OR atttypmod<>-1 OR attcollation<>typcollation
       OR NOT ((name='id' AND atttypid='pg_catalog.text'::regtype)
            OR (name='data' AND atttypid='pg_catalog.bytea'::regtype)
            OR (name='expiry_date' AND atttypid='pg_catalog.timestamptz'::regtype))
  )
  AND EXISTS (
    SELECT 1 FROM pg_catalog.pg_index i JOIN relation r ON r.oid=i.indrelid
    JOIN columns c ON c.name='id'
    WHERE i.indisprimary AND i.indisunique AND i.indisvalid AND i.indisready
      AND i.indimmediate AND i.indnkeyatts=1 AND i.indkey[0]=c.attnum
  )
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_constraint k JOIN relation r ON r.oid=k.conrelid
    WHERE k.contype NOT IN ('p','n') OR NOT k.convalidated OR NOT k.conenforced
       OR k.condeferrable OR k.condeferred
  )
  -- Read actual rows, not lazily maintained relhastriggers/relhasrules flags.
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t JOIN relation r ON r.oid=t.tgrelid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite w JOIN relation r ON r.oid=w.ev_class)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_policy p JOIN relation r ON r.oid=p.polrelid)
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_inherits i JOIN relation r ON r.oid IN (i.inhrelid,i.inhparent)
  )
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_index i JOIN relation r ON r.oid=i.indrelid
    JOIN pg_catalog.pg_class c ON c.oid=i.indexrelid
    JOIN pg_catalog.pg_am am ON am.oid=c.relam
    WHERE (i.indisunique AND NOT i.indisprimary) OR i.indisexclusion
       OR NOT i.indisvalid OR NOT i.indisready OR NOT i.indimmediate
       OR i.indexprs IS NOT NULL OR i.indpred IS NOT NULL OR am.amname<>'btree'
       OR EXISTS (
         SELECT 1 FROM generate_series(0,i.indnkeyatts-1) AS position
         LEFT JOIN columns a ON a.attnum=i.indkey[position]
         LEFT JOIN pg_catalog.pg_opclass op ON op.oid=i.indclass[position]
         WHERE a.attnum IS NULL OR op.oid IS NULL OR NOT op.opcdefault
            OR op.opcnamespace<>'pg_catalog'::regnamespace OR op.opcintype<>a.atttypid
            OR i.indcollation[position]<>a.attcollation
       )
  );

-- Native PostgreSQL privilege inspection, not a sampled table allowlist.
-- Include session_user: SET ROLE must not hide an elevated login identity.
WITH reachable_roles AS (
  SELECT r.* FROM pg_catalog.pg_roles r
  WHERE pg_catalog.pg_has_role(current_user, r.oid, 'USAGE')
     OR pg_catalog.pg_has_role(current_user, r.oid, 'SET')
     OR pg_catalog.pg_has_role(session_user, r.oid, 'USAGE')
     OR pg_catalog.pg_has_role(session_user, r.oid, 'SET')
), service_schemas AS (
  SELECT oid, nspowner FROM pg_catalog.pg_namespace WHERE nspname IN ('app','tower_sessions','pgmq')
)
SELECT (SELECT count(*) FROM service_schemas) <> 3 OR EXISTS (
  SELECT 1 FROM reachable_roles r
  WHERE r.rolsuper OR r.rolcreaterole OR r.rolcreatedb OR r.rolbypassrls OR r.rolreplication
     OR EXISTS (
       SELECT 1 FROM pg_catalog.pg_database d
       WHERE d.datname = pg_catalog.current_database() AND d.datdba = r.oid
     )
     OR EXISTS (
       SELECT 1 FROM service_schemas n
       WHERE n.nspowner = r.oid
          OR pg_catalog.has_schema_privilege(r.oid, n.oid, 'CREATE')
     )
     OR EXISTS (
       SELECT 1 FROM pg_catalog.pg_class c
       JOIN service_schemas n ON n.oid = c.relnamespace
       WHERE c.relowner = r.oid
          OR (c.relkind IN ('r','p','v','m','f')
              AND pg_catalog.has_table_privilege(r.oid, c.oid, 'TRUNCATE,TRIGGER'))
     )
     OR EXISTS (
       SELECT 1 FROM pg_catalog.pg_proc p
       JOIN service_schemas n ON n.oid = p.pronamespace
       WHERE p.proowner = r.oid
     )
     OR EXISTS (
       SELECT 1 FROM pg_catalog.pg_type t
       JOIN service_schemas n ON n.oid = t.typnamespace
       WHERE t.typowner = r.oid
     )
)

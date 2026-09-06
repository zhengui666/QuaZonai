-- Native PostgreSQL privilege inspection, not a sampled table allowlist.
-- Include session_user: SET ROLE must not hide an elevated login identity.
-- ADMIN OPTION is authority even with INHERIT/SET disabled: its holder can
-- re-grant membership with SET TRUE. Follow the native grant graph, including
-- multi-hop administrative delegation. Plain membership with all flags false
-- does not confer this authority. UNION deduplicates identities.
WITH RECURSIVE reachable_role_ids(oid) AS (
  SELECT oid FROM pg_catalog.pg_roles WHERE rolname IN (current_user, session_user)
  UNION
  SELECT membership.roleid
  FROM reachable_role_ids held
  JOIN pg_catalog.pg_auth_members membership ON membership.member = held.oid
  WHERE membership.inherit_option OR membership.set_option OR membership.admin_option
), reachable_roles AS (
  SELECT role.* FROM pg_catalog.pg_roles role JOIN reachable_role_ids held USING (oid)
), service_schemas AS (
  SELECT oid, nspowner FROM pg_catalog.pg_namespace WHERE nspname IN ('app','tower_sessions','pgmq')
)
SELECT (SELECT count(*) FROM service_schemas) <> 3 OR EXISTS (
  SELECT 1 FROM reachable_roles r
  WHERE r.rolsuper OR r.rolcreaterole OR r.rolcreatedb OR r.rolbypassrls OR r.rolreplication
     -- PostgreSQL documents these as bypassing database-level protection and
     -- potentially granting superuser-equivalent server access.
     OR r.rolname IN ('pg_read_server_files','pg_write_server_files','pg_execute_server_program')
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

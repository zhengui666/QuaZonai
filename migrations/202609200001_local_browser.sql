-- Local operation replaces user enrollment. Existing records and encrypted
-- historical material stay intact; no legacy authority is resurrected.
ALTER TABLE app.operator_auth_state DROP CONSTRAINT operator_auth_state_check;
UPDATE app.operator_auth_state
 SET initialized=true, setup_completed_at=coalesce(setup_completed_at,clock_timestamp()),
     session_epoch=greatest(
       session_epoch,
       coalesce((SELECT max(auth_epoch) FROM app.browser_logins),0),
       coalesce((SELECT max(auth_epoch) FROM app.trusted_devices),0),
       coalesce((SELECT max(auth_epoch) FROM app.operator_command_grants),0)
     )+1
 WHERE singleton;
ALTER TABLE app.operator_auth_state ADD CONSTRAINT local_operator_ready
 CHECK (initialized AND setup_completed_at IS NOT NULL);

-- Separate role/profile identities and Threads share the owner's one native
-- Codex configuration. Neither registration overwrites config.toml or auth.json.
INSERT INTO app.codex_profiles(name,connection_mode,profile_origin,codex_home_ref,
 use_default_model_settings,saved_fast_mode)
VALUES ('研究员','SYSTEM','OPERATOR_MOUNT','local-researcher',true,false),
       ('独立审阅员','SYSTEM','OPERATOR_MOUNT','local-reviewer',true,false)
ON CONFLICT (codex_home_ref) DO NOTHING;

-- A downstream native claim identity cannot identify two original transfers.
-- Incompatible historical duplicates fail migration without rewriting history.
CREATE UNIQUE INDEX downstream_original_claim ON app.handoff_transfers(downstream_id,external_claim_id);
CREATE INDEX handoff_pending_expiry ON app.handoff_offers(expires_at,id) WHERE state='OFFERED';

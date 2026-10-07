-- NULL is an explicitly absent application token cap, never a sentinel grant.
-- Historical bounded reservations and their immutable receipts are preserved.
ALTER TABLE app.model_turn_reservations
    ALTER COLUMN reserved_tokens DROP NOT NULL;

COMMENT ON COLUMN app.model_turn_reservations.reserved_tokens IS
    'Optional application token cap; NULL reserves no token amount. Actual usage remains in immutable receipts.';

-- The existing positive-value CHECK permits NULL and still rejects zero or
-- negative bounded caps. The existing accounting view preserves NULL while an
-- uncapped turn is unresolved, and its reserved_turns remains one. Settlement
-- continues to require an authoritative receipt with non-null actual_tokens;
-- a missing receipt is not a zero-usage settlement.

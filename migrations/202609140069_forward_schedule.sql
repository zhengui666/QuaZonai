-- Mutable scheduling hints, never evidence or authorization. Original Run admission remains authoritative.
CREATE TABLE app.forward_schedule (
    handoff_id app.identity NOT NULL REFERENCES app.handoff_offers,
    stream_id app.nonempty NOT NULL,
    observed_message_count app.counter NOT NULL CHECK(observed_message_count>0),
    last_attempt_at app.instant NOT NULL,
    next_attempt_at app.instant NOT NULL,
    PRIMARY KEY(handoff_id,stream_id),
    CHECK(next_attempt_at>last_attempt_at)
);

-- Append-only audit trail of every state transition and step execution.
-- Never updated or deleted; used for debugging, post-mortems, and replay.
CREATE TABLE IF NOT EXISTS workflow_events (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    position_id  UUID        NOT NULL REFERENCES positions (id),
    event_type   TEXT        NOT NULL,   -- 'state_transition' | 'step_started' | 'step_completed' | 'step_failed'
    from_state   TEXT,
    to_state     TEXT,
    step         TEXT,
    tx_hash      TEXT,
    metadata     JSONB,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_workflow_events_position_id ON workflow_events (position_id);
CREATE INDEX IF NOT EXISTS idx_workflow_events_created_at  ON workflow_events (created_at DESC);

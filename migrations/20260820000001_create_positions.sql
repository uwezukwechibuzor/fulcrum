-- Positions: the core financial state.
--
-- version: optimistic concurrency control. Every UPDATE increments version
-- and adds `AND version = $old_version` to the WHERE clause; zero rows
-- affected means another writer raced us and we must retry.
CREATE TABLE IF NOT EXISTS positions (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_address    TEXT        NOT NULL,
    rwa_token        TEXT        NOT NULL,
    facility         TEXT        NOT NULL,
    market_id        TEXT        NOT NULL,
    target_leverage  NUMERIC(10, 4) NOT NULL,

    -- State machine
    state            TEXT        NOT NULL DEFAULT 'opening'
                                 CHECK (state IN ('opening','live','rebalancing','closing','closed','failed')),
    current_step     TEXT,       -- NULL when not mid-workflow

    -- Financial snapshot (updated after each step confirms on-chain)
    collateral_amount NUMERIC(36, 18),
    debt_amount       NUMERIC(36, 18),
    health_factor     NUMERIC(10, 4),

    -- Last submitted transaction; used for idempotent step resume
    last_tx_hash     TEXT,

    error_message    TEXT,
    version          BIGINT      NOT NULL DEFAULT 0,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_positions_state         ON positions (state);
CREATE INDEX IF NOT EXISTS idx_positions_owner_address ON positions (owner_address);

-- Trigger: keep updated_at fresh automatically.
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER positions_updated_at
    BEFORE UPDATE ON positions
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

----------------------------------------------------------------
-- v8: Compact address to transactions using script public key
----------------------------------------------------------------

CREATE TABLE scripts_transactions
(
    script_public_key BYTEA,
    transaction_id    BYTEA,
    block_time        BIGINT,
    PRIMARY KEY (script_public_key, transaction_id)
);
CREATE INDEX ON scripts_transactions (script_public_key, block_time DESC);


-- Update schema_version
UPDATE vars SET value = '8' WHERE key = 'schema_version';

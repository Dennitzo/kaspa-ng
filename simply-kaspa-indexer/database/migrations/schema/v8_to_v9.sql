--------------------------------------------------------------
-- v9: Support resolving input previous_outpoint
--------------------------------------------------------------

-- Remove NOT NULL on transactions_acceptances.transaction_id (missing in v6_to_v7.sql)
ALTER TABLE transactions_acceptances ALTER COLUMN transaction_id DROP NOT NULL;

-- Add previous_outpoint_script, previous_outpoint_amount to transactions_inputs
ALTER TABLE transactions_inputs ADD COLUMN previous_outpoint_script BYTEA;
ALTER TABLE transactions_inputs ADD COLUMN previous_outpoint_amount BIGINT;

-- Create indexes (optional, to look up addresses without addresses_transactions)
--CREATE INDEX ON transactions_inputs (previous_outpoint_script, block_time DESC);
--CREATE INDEX ON transactions_outputs (script_public_key, block_time DESC);

-- Update schema_version
UPDATE vars SET value = '9' WHERE key = 'schema_version';

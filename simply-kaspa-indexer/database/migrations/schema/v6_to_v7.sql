--------------------------------------------------------------
-- v7: Support block-only mode
--------------------------------------------------------------
--     THIS WILL TAKE A WHILE!
--------------------------------------------------------------

ALTER TABLE transactions_acceptances DROP CONSTRAINT transactions_acceptances_pkey;
ALTER TABLE transactions_acceptances ADD UNIQUE (transaction_id);

-- Update schema_version
UPDATE vars SET value = '7' WHERE key = 'schema_version';

-- The legacy link table becomes a link with one secret, `token`, sealed the
-- same way (the user ID as associated data), so it moves as is.
INSERT INTO links (user_id, status, token_hash, last_ok_at, last_error, created_at, updated_at)
SELECT user_id, status, token_hash, last_ok_at, last_error, updated_at, updated_at
FROM hackmd_links;
INSERT INTO link_secrets (user_id, name, sealed)
SELECT user_id, 'token', token_sealed FROM hackmd_links;
DROP TABLE hackmd_links;

-- Adopts a hackmd-mcp database from before mcp-backbone: reshapes the
-- tables the framework shares with it, keeping every row. Runs once, in one
-- transaction, before the framework's baseline creates the tables missing.
ALTER TABLE users RENAME COLUMN hackmd_user_id TO subject;
ALTER TABLE users RENAME CONSTRAINT users_hackmd_user_id_key TO users_subject_key;
ALTER TABLE users RENAME COLUMN user_path TO login;
ALTER TABLE users ALTER COLUMN name DROP NOT NULL;
ALTER TABLE users ADD COLUMN profile jsonb NOT NULL DEFAULT '{}';

ALTER TABLE agent_sessions RENAME COLUMN workspace_scope TO scope;
ALTER TABLE agent_sessions ALTER COLUMN scope SET DEFAULT '{}';
ALTER TABLE agent_sessions ADD COLUMN use_outbox boolean NOT NULL DEFAULT false;
ALTER TABLE agent_sessions ADD COLUMN outbox_ttl_minutes integer NOT NULL DEFAULT 60
    CHECK (outbox_ttl_minutes BETWEEN 1 AND 10080);

ALTER TABLE tool_calls ADD COLUMN original_arguments jsonb;
ALTER TABLE tool_calls ADD COLUMN outbox_expires_at timestamptz;
ALTER TABLE tool_calls ADD COLUMN decided_at timestamptz;
ALTER TABLE tool_calls DROP CONSTRAINT tool_calls_status_check;
ALTER TABLE tool_calls ADD CONSTRAINT tool_calls_status_check CHECK (status IN
    ('running', 'ok', 'error', 'denied', 'queued', 'discarded', 'expired'));

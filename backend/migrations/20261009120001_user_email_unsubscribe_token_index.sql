-- no-transaction
--
-- Unique index on `users.email_unsubscribe_token`, so the unsubscribe endpoint finds the user
-- without scanning `users` and no two users share a token. NULLs are distinct, so users without a
-- token do not conflict. Outside a transaction because the index is built `CONCURRENTLY`.

SET lock_timeout = '5s';
SET statement_timeout = '0';

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_users_email_unsubscribe_token
    ON users (email_unsubscribe_token);

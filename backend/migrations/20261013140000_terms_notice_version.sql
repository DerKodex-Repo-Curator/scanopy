-- Terms-update notices: the newest terms version each user has been emailed notice of.
--
-- `users.terms_notice_version` is the effective date of the newest terms update the cloud
-- server has emailed this user about. The daily terms-notice sweep emails users whose value
-- is NULL or older than a pending update, and advances it after each send, so a user gets one
-- notice per version. Proof of who was sent what lives in the SES event archive, not here.
--
-- Expand only: one nullable column, nothing dropped or rewritten.

SET lock_timeout = '5s';
SET statement_timeout = '5s';

ALTER TABLE users ADD COLUMN IF NOT EXISTS terms_notice_version DATE;

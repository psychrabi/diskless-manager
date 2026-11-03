-- Consecutive failed spare builds or cleanup attempts per client. The counter is read by the
-- alarm read model so repeated prewarm failures raise an alarm without a
-- second write path; a successful build resets it to zero.
ALTER TABLE client_writeback_spares ADD COLUMN failures INTEGER NOT NULL DEFAULT 0;

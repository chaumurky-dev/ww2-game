-- Defenders never wait on a human: the server resolves their battle decisions from these
-- per-user preferences (see wartide_engine::casualty_policy).
ALTER TABLE users
    ADD COLUMN casualty_policy TEXT NOT NULL DEFAULT 'cheapest_first',
    ADD COLUMN sub_withdrawal TEXT NOT NULL DEFAULT 'when_outgunned';

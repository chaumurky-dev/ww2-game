-- Game names
ALTER TABLE games ADD COLUMN name TEXT NOT NULL DEFAULT '';
CREATE SEQUENCE public_game_number START 1000;
UPDATE games SET name = CASE kind
    WHEN 'public' THEN 'Ranked ' || (CASE mode WHEN 'two_player' THEN '2p' ELSE '5p' END) || ' #' || upper(substr(replace(id::text, '-', ''), 1, 4))
    ELSE 'Private game #' || upper(substr(replace(id::text, '-', ''), 1, 4))
END;
CREATE INDEX games_public_listing_idx ON games (kind, status, created_at DESC);

-- Bot personas are now stored by catalog id instead of display name
UPDATE game_seats SET bot_persona = CASE bot_persona
    WHEN 'Colonel Brass' THEN 'brass'
    WHEN 'General Frost' THEN 'frost'
    WHEN 'Admiral Kestrel' THEN 'kestrel'
    WHEN 'Marshal Volkov' THEN 'volkov'
    WHEN 'Commodore Tide' THEN 'tide'
    ELSE bot_persona
END WHERE bot_persona IS NOT NULL;

-- Users: admin flag (set via SQL only), match-found email toggle, soft delete
ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE users ADD COLUMN email_match_found BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE users ADD COLUMN deleted_at TIMESTAMPTZ;

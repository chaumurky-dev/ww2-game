-- Private games may use any seat layout ("custom"); the layout itself lives in game_seats.
ALTER TABLE games DROP CONSTRAINT IF EXISTS games_mode_check;
ALTER TABLE games ADD CONSTRAINT games_mode_check CHECK (mode IN ('two_player', 'five_player', 'custom'));

-- Engine integration: richer settings, seat bookkeeping, denormalised turn info.

-- Settings: economic_victory(+threshold) became axis_economic_victory (null = off).
UPDATE games SET settings = (settings - 'economic_victory' - 'economic_victory_threshold') ||
    jsonb_build_object('axis_economic_victory',
        CASE WHEN (settings->>'economic_victory')::boolean IS TRUE
             THEN COALESCE(settings->'economic_victory_threshold', '84'::jsonb)
             ELSE 'null'::jsonb END)
WHERE settings ? 'economic_victory' OR settings ? 'economic_victory_threshold';

-- Games that ran on the pre-engine placeholder state cannot continue.
UPDATE games SET status = 'abandoned', turn_deadline = NULL
WHERE status = 'in_progress'
  AND id IN (SELECT game_id FROM game_states WHERE state ? 'placeholder');

ALTER TABLE games ADD COLUMN phase TEXT;
ALTER TABLE games ADD COLUMN pending_power TEXT;

-- timeouts = turns missed in total, missed_in_row = consecutive (reset by any human action),
-- resigned = a human left and a bot plays the seat (the user stays attached for ratings).
ALTER TABLE game_seats ADD COLUMN timeouts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE game_seats ADD COLUMN missed_in_row INTEGER NOT NULL DEFAULT 0;
ALTER TABLE game_seats ADD COLUMN resigned BOOLEAN NOT NULL DEFAULT FALSE;

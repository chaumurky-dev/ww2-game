CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL,
    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    avatar TEXT,
    bio TEXT,
    country TEXT,
    timezone TEXT,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    email_turn_notifications BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_lower_idx ON users (lower(email));
CREATE UNIQUE INDEX users_username_lower_idx ON users (lower(username));

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

-- one-time tokens for email verification and password reset (stored hashed)
CREATE TABLE email_tokens (
    token_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose TEXT NOT NULL CHECK (purpose IN ('verify', 'reset')),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ
);

CREATE TABLE groups (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT,
    invite_code TEXT NOT NULL UNIQUE,
    owner_id UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE group_members (
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX group_members_user_idx ON group_members (user_id);

CREATE TABLE ratings (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    ladder TEXT NOT NULL,
    rating DOUBLE PRECISION NOT NULL DEFAULT 1500,
    deviation DOUBLE PRECISION NOT NULL DEFAULT 350,
    volatility DOUBLE PRECISION NOT NULL DEFAULT 0.06,
    games INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, ladder)
);
CREATE INDEX ratings_ladder_idx ON ratings (ladder, rating DESC);

CREATE TABLE games (
    id UUID PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('private', 'public')),
    mode TEXT NOT NULL CHECK (mode IN ('two_player', 'five_player')),
    status TEXT NOT NULL DEFAULT 'lobby' CHECK (status IN ('lobby', 'in_progress', 'finished', 'abandoned')),
    settings JSONB NOT NULL,
    group_id UUID REFERENCES groups(id) ON DELETE SET NULL,
    host_id UUID REFERENCES users(id) ON DELETE SET NULL,
    -- rating ladder this game counts for (NULL = unrated)
    ladder TEXT,
    seed BIGINT NOT NULL,
    round INTEGER NOT NULL DEFAULT 0,
    current_power TEXT,
    turn_deadline TIMESTAMPTZ,
    winner_team TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX games_status_deadline_idx ON games (status, turn_deadline);

CREATE TABLE game_seats (
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    seat INTEGER NOT NULL,
    powers TEXT[] NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    is_bot BOOLEAN NOT NULL DEFAULT FALSE,
    bot_persona TEXT,
    PRIMARY KEY (game_id, seat)
);
CREATE INDEX game_seats_user_idx ON game_seats (user_id);

-- append-only action log
CREATE TABLE game_actions (
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    actor_seat INTEGER,
    action JSONB NOT NULL,
    events JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (game_id, seq)
);

CREATE TABLE game_states (
    game_id UUID PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    state JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE game_invites (
    code TEXT PRIMARY KEY,
    game_id UUID NOT NULL UNIQUE REFERENCES games(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE chat_messages (
    id UUID PRIMARY KEY,
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    channel TEXT NOT NULL CHECK (channel IN ('all', 'team', 'power', 'lobby')),
    -- team channel: 'axis'|'allies'; power channel: power name
    scope TEXT,
    sender_kind TEXT NOT NULL CHECK (sender_kind IN ('user', 'bot', 'system')),
    sender_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    sender_name TEXT,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX chat_game_idx ON chat_messages (game_id, created_at);

CREATE TABLE matchmaking_queue (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    mode TEXT NOT NULL CHECK (mode IN ('two_player', 'five_player')),
    rating DOUBLE PRECISION NOT NULL,
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

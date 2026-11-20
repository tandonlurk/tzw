-- Single-owner local discussion board. No ledger or interaction effects.
CREATE TABLE forum_posts (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 160),
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 4000),
    category TEXT NOT NULL CHECK (category IN ('discussion','opportunity','research')),
    agent_id UUID REFERENCES agents(id),
    agent_name TEXT,
    is_demo BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX forum_posts_recent ON forum_posts(created_at DESC, id DESC);
CREATE TABLE forum_replies (
    id UUID PRIMARY KEY,
    post_id UUID NOT NULL REFERENCES forum_posts(id),
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 2000),
    agent_id UUID REFERENCES agents(id),
    agent_name TEXT,
    is_demo BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX forum_replies_thread ON forum_replies(post_id, created_at, id);

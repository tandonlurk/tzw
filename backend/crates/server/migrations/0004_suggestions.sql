-- Co-captain suggestions. Advisory only: a row here never moves funds and
-- never authorizes anything. Accepting one with a `proposed_change` goes
-- through the same validated, versioned agent edit as the owner's own edit.

CREATE TABLE suggestions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    seq             BIGSERIAL NOT NULL UNIQUE,
    agent_id        UUID REFERENCES agents (id),
    kind            TEXT NOT NULL,
    -- What the suggestion is about (an approval id, a version, a settle
    -- count ...). The same subject is never suggested twice, so a dismissed
    -- suggestion does not come back until the underlying data changes.
    subject         TEXT NOT NULL,
    title           TEXT NOT NULL,
    body            TEXT NOT NULL,
    -- cocaptain::ProposedChange, or NULL for a text-only suggestion.
    proposed_change JSONB,
    -- The real numbers the suggestion was derived from.
    evidence        JSONB NOT NULL,
    -- Computed in Rust from `proposed_change`, never taken from a model.
    loosens_control BOOLEAN NOT NULL DEFAULT FALSE,
    source          TEXT NOT NULL CHECK (source IN ('rules', 'grok')),
    status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'dismissed', 'expired')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_at      TIMESTAMPTZ,
    CHECK (proposed_change IS NULL OR agent_id IS NOT NULL),
    CHECK ((status = 'pending') = (decided_at IS NULL))
);

-- At most one pending suggestion of a kind per agent (NULL agent = the owner).
CREATE UNIQUE INDEX suggestions_one_pending_per_kind
    ON suggestions (COALESCE(agent_id, '00000000-0000-0000-0000-000000000000'::uuid), kind)
    WHERE status = 'pending';
CREATE UNIQUE INDEX suggestions_subject_once
    ON suggestions (COALESCE(agent_id, '00000000-0000-0000-0000-000000000000'::uuid), kind, subject);
CREATE INDEX suggestions_status_idx ON suggestions (status, seq DESC);

-- A decision is final: only pending -> accepted / dismissed (by the owner) or
-- expired (the condition went away before the owner decided), nothing else
-- about a row ever changes and rows are never deleted.
CREATE FUNCTION suggestions_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'suggestions are never deleted';
    END IF;
    IF OLD.status <> 'pending' THEN
        RAISE EXCEPTION 'a decided suggestion is immutable';
    END IF;
    IF (NEW.id, NEW.seq, NEW.agent_id, NEW.kind, NEW.subject, NEW.title, NEW.body,
        NEW.proposed_change, NEW.evidence, NEW.loosens_control, NEW.source, NEW.created_at)
       IS DISTINCT FROM
       (OLD.id, OLD.seq, OLD.agent_id, OLD.kind, OLD.subject, OLD.title, OLD.body,
        OLD.proposed_change, OLD.evidence, OLD.loosens_control, OLD.source, OLD.created_at) THEN
        RAISE EXCEPTION 'only the status of a suggestion may change';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER suggestions_guard
    BEFORE UPDATE OR DELETE ON suggestions
    FOR EACH ROW EXECUTE FUNCTION suggestions_guard();

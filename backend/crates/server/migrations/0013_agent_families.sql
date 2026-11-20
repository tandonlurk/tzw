-- Agent multiplier (D-73): the owner clones one agent N times with bounded,
-- seeded parameter variance. Each clone is an ordinary agent (own id,
-- versions, simulated allocation); these two tables only record where it
-- came from, so the result is reproducible and auditable. Simulated only.
--
-- A family is identified by its root agent's id: the agent that was first
-- multiplied. Clones of a clone join the root's family. The root itself has
-- no row in agent_clones.

CREATE TABLE agent_multiplications (
    id              UUID PRIMARY KEY,
    family_id       UUID NOT NULL REFERENCES agents (id),
    source_agent_id UUID NOT NULL REFERENCES agents (id),
    source_version  INT NOT NULL CHECK (source_version >= 1),
    count           INT NOT NULL CHECK (count BETWEEN 1 AND 100),
    variance_bps    INT NOT NULL CHECK (variance_bps BETWEEN 0 AND 2500),
    seed            BIGINT NOT NULL CHECK (seed BETWEEN 0 AND 4294967295),
    budget          BIGINT NOT NULL CHECK (budget > 0),
    per_clone       BIGINT NOT NULL CHECK (per_clone > 0),
    env             TEXT NOT NULL CHECK (env = 'simulated'),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (per_clone::NUMERIC * count <= budget)
);

CREATE TABLE agent_clones (
    agent_id          UUID PRIMARY KEY REFERENCES agents (id),
    family_id         UUID NOT NULL REFERENCES agents (id),
    multiplication_id UUID NOT NULL REFERENCES agent_multiplications (id),
    clone_index       INT NOT NULL CHECK (clone_index >= 1),
    -- {variance_bps, seed, source_version, params: [{path, from, to, offset_bps}]}
    variance          JSONB NOT NULL,
    UNIQUE (family_id, clone_index)
);

CREATE INDEX agent_clones_family_idx ON agent_clones (family_id);

CREATE TRIGGER agent_multiplications_immutable
    BEFORE UPDATE OR DELETE ON agent_multiplications
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER agent_clones_immutable
    BEFORE UPDATE OR DELETE ON agent_clones
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER agent_multiplications_no_truncate BEFORE TRUNCATE ON agent_multiplications
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_truncate();
CREATE TRIGGER agent_clones_no_truncate BEFORE TRUNCATE ON agent_clones
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_truncate();

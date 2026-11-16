-- SEC-08 / SEC-09: the database, not only the app, keeps the history
-- append-only and every posting set balanced; idempotency keys are scoped
-- per environment.

-- SEC-08a: the row-level `forbid_mutation` triggers do not fire on TRUNCATE.
-- A statement-level trigger refuses it on every append-only table. The test
-- harness (and only it) opts out per transaction with
-- `SET LOCAL aen.allow_truncate = 'on'` before wiping its database. This
-- guards against accidents and app bugs, not against a superuser, who can
-- drop the trigger; see the least-privilege role note in backend/README.md.
CREATE FUNCTION forbid_truncate() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF current_setting('aen.allow_truncate', true) = 'on' THEN
        RETURN NULL;
    END IF;
    RAISE EXCEPTION '% is append-only: TRUNCATE is refused', TG_TABLE_NAME;
END
$$;

CREATE TRIGGER ledger_entries_no_truncate BEFORE TRUNCATE ON ledger_entries
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_truncate();
CREATE TRIGGER agent_versions_no_truncate BEFORE TRUNCATE ON agent_versions
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_truncate();
CREATE TRIGGER negotiation_rounds_no_truncate BEFORE TRUNCATE ON negotiation_rounds
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_truncate();
CREATE TRIGGER suggestions_no_truncate BEFORE TRUNCATE ON suggestions
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_truncate();

-- SEC-09: an idempotency key is unique per environment, not globally.
-- (Client-supplied keys are additionally namespaced per caller by the app,
-- so they can never collide with the lifecycle's own keys.)
ALTER TABLE ledger_entries DROP CONSTRAINT ledger_entries_idempotency_key_line_key;
ALTER TABLE ledger_entries ADD CONSTRAINT ledger_entries_env_key_line_key
    UNIQUE (env, idempotency_key, line);

-- SEC-08b: every posting set (rows sharing env and idempotency key) sums to
-- zero when the transaction commits. Deferred, so the rows of one set can be
-- inserted one by one inside a transaction.
CREATE FUNCTION ledger_set_balanced() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    total NUMERIC;
BEGIN
    SELECT SUM(delta) INTO total FROM ledger_entries
     WHERE env = NEW.env AND idempotency_key = NEW.idempotency_key;
    IF total <> 0 THEN
        RAISE EXCEPTION 'ledger posting set % (%) does not sum to zero (%)',
            NEW.idempotency_key, NEW.env, total;
    END IF;
    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER ledger_entries_balanced
    AFTER INSERT ON ledger_entries
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION ledger_set_balanced();

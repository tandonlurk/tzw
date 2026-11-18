-- Starter templates get an `approach.profile` node (TODO M3b, D-69), so a new
-- agent opens with a stance on the approach wheel and the ambition sizing of
-- D-64 applies. Agents already created keep their own graph (templates are
-- copied at creation). Markets stay "any" so the templates work on every
-- market source.
--
--   Momentum bull: competitive, fully ambitious (stake target = max per
--                  interaction, min conviction 50 %, up to 5 open deals).
--   Skeptic:       competitive, middle ambition. It only answers proposals,
--                  and the sizing applies to proposals it makes, so in
--                  practice the stance is what the owner sees.
--   Cautious:      collaboration 30 % (may join firms if the owner sets a
--                  role), ambition 20 % (stake target 36 % of max per
--                  interaction, min conviction 70 %, 1 open deal).

UPDATE templates
SET graph = jsonb_set(graph, '{nodes}', (graph -> 'nodes') || jsonb_build_array(jsonb_build_object(
    'id', 'approach',
    'type', 'approach.profile',
    'params', jsonb_build_object(
        'stance', jsonb_build_object('collaboration_bps', v.collab, 'ambition_bps', v.ambition),
        'firm_role', 'none',
        'markets', jsonb_build_object('scope', 'any')
    ),
    'ui', jsonb_build_object('x', 0, 'y', 360)
)))
FROM (VALUES
    ('00000000-0000-4000-8000-000000000001'::uuid, 1500, 10000),
    ('00000000-0000-4000-8000-000000000002'::uuid, 2500, 5000),
    ('00000000-0000-4000-8000-000000000003'::uuid, 3000, 2000)
) AS v(id, collab, ambition)
WHERE templates.id = v.id
  AND NOT EXISTS (
      SELECT 1 FROM jsonb_array_elements(templates.graph -> 'nodes') n
      WHERE n ->> 'type' = 'approach.profile'
  );

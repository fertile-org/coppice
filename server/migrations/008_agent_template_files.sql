ALTER TABLE agent_presets DROP COLUMN system_prompt_template;

UPDATE agent_presets SET
  skills = ARRAY['planning','requirements','prioritization','decomposition','assignment'],
  responsibilities = ARRAY['refine ticket scope','split oversized work','recommend agent assignment','escalate blockers','synthesize cross-ticket status']
WHERE key = 'pm';

UPDATE agent_presets SET
  skills = ARRAY['architecture','system design','technical review','tradeoff analysis'],
  responsibilities = ARRAY['guide implementation approach','review designs and significant changes','flag architectural risk']
WHERE key = 'tech_lead';

UPDATE agent_presets SET
  skills = ARRAY['UI implementation','component design','accessibility','frontend testing'],
  responsibilities = ARRAY['implement frontend tickets','follow project UI conventions','fix UI defects','raise frontend tech debt']
WHERE key = 'frontend_engineer';

UPDATE agent_presets SET
  skills = ARRAY['API design','services','persistence','backend testing'],
  responsibilities = ARRAY['implement backend tickets','follow project service conventions','fix backend defects','raise backend tech debt']
WHERE key = 'backend_engineer';

UPDATE agent_presets SET
  skills = ARRAY['postgres','schema design','migrations','query performance'],
  responsibilities = ARRAY['review schema changes','inspect query and migration risk','suggest index and data safety improvements']
WHERE key = 'dba';

UPDATE agent_presets SET
  skills = ARRAY['testing','QA','regression analysis','acceptance criteria'],
  responsibilities = ARRAY['verify ticket acceptance criteria','design and run test scenarios','report defects with reproduction steps']
WHERE key = 'qc';

UPDATE agent_presets SET
  skills = ARRAY['code review','diff analysis','maintainability'],
  responsibilities = ARRAY['review changes for correctness and scope','request fixes','approve when standards are met']
WHERE key = 'reviewer';

UPDATE agent_presets SET
  skills = ARRAY['CI/CD','containers','deployment','observability'],
  responsibilities = ARRAY['maintain pipelines and deploy paths','diagnose build/deploy failures','suggest operational improvements']
WHERE key = 'devops';

UPDATE agent_presets SET
  skills = ARRAY['threat modeling','dependency audit','secure coding'],
  responsibilities = ARRAY['review changes for security risk','flag vulnerabilities and unsafe patterns','recommend mitigations']
WHERE key = 'security';

UPDATE agent_presets SET
  skills = ARRAY['investigation','technical spikes','comparative analysis'],
  responsibilities = ARRAY['explore unknowns','summarize findings with sources','recommend follow-up tickets']
WHERE key = 'research';

-- The built-in skills tree lives at <builtin_plugins_dir>/coppice/skills and was
-- scanned as a skills-only plugin named "coppice", which check_name rejects as
-- reserved. Discovery now skips that folder; delete the stale invalid row so
-- existing installs do not keep showing it as "Needs attention" / missing.
DELETE FROM plugins
WHERE name = 'coppice' AND rel_path = 'coppice' AND status = 'invalid';

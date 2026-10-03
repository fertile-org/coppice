-- A plugin_settings row owns its secret; deleting the row (directly or via the
-- plugins cascade) deletes the secret so no ciphertext outlives its plugin.
CREATE OR REPLACE FUNCTION delete_plugin_setting_secret()
RETURNS trigger AS $$
BEGIN
    DELETE FROM secrets WHERE id = OLD.secret_id;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER plugin_settings_delete_secret
AFTER DELETE ON plugin_settings
FOR EACH ROW EXECUTE FUNCTION delete_plugin_setting_secret();

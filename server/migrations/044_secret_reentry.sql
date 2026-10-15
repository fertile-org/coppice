-- A backup can restore ciphertext this install cannot decrypt. Keep the row,
-- clear the ciphertext, and ask for the token again.
ALTER TABLE secrets
    ADD COLUMN needs_reentry BOOLEAN NOT NULL DEFAULT false;

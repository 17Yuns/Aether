ALTER TABLE user_groups
    ADD COLUMN IF NOT EXISTS billing_multiplier double precision NULL;

ALTER TABLE public.api_keys
    ADD COLUMN IF NOT EXISTS billing_multiplier double precision NOT NULL DEFAULT 1.0;

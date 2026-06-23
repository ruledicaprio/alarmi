-- v7 schema additions:
--   1. user_role_t enum + dim_user table (no auth enforcement yet — LDAP later)
--   2. dim_region_canonical (the 7 official BHT regions, used by UI filters)
--   3. v_verified_inventory view (sites with latest verification, surfaced as is_verified)
-- Idempotent.

\set ON_ERROR_STOP on

-- 1. role enum
DO $$ BEGIN
  CREATE TYPE user_role_t AS ENUM ('superadmin', 'admin', 'user');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- 2. dim_user — operator/audit identity. No password (LDAP stub later).
CREATE TABLE IF NOT EXISTS dim_user (
    id          BIGSERIAL PRIMARY KEY,
    username    TEXT NOT NULL UNIQUE,
    full_name   TEXT NOT NULL DEFAULT '',
    role        user_role_t NOT NULL DEFAULT 'user',
    region      TEXT DEFAULT NULL,             -- restrict view to a region (NULL = all)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen   TIMESTAMPTZ DEFAULT NULL,
    disabled    BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS ix_user_username ON dim_user (username);

-- seed two starter accounts so admin UI isn't empty
INSERT INTO dim_user (username, full_name, role)
VALUES ('rusmir', 'Rusmir Skopljak', 'superadmin')
ON CONFLICT (username) DO NOTHING;

-- 3. canonical regions per BHT operator spec
CREATE TABLE IF NOT EXISTS dim_region_canonical (
    region    TEXT PRIMARY KEY,
    sort_idx  INT  NOT NULL,
    label     TEXT NOT NULL
);
INSERT INTO dim_region_canonical (region, sort_idx, label) VALUES
    ('SARAJEVO', 1, 'Sarajevo'),
    ('TUZLA',    2, 'Tuzla'),
    ('ZENICA',   3, 'Zenica'),
    ('BIHAC',    4, 'Bihać'),
    ('MOSTAR',   5, 'Mostar'),
    ('TRAVNIK',  6, 'Travnik'),
    ('GORAZDE',  7, 'Goražde')
ON CONFLICT (region) DO UPDATE SET
    sort_idx = EXCLUDED.sort_idx,
    label    = EXCLUDED.label;

-- 4. verified_inventory view — every site joined with its latest verification record
CREATE OR REPLACE VIEW v_verified_inventory AS
SELECT
    s.site_key,
    COALESCE(s.display_name, '') AS display_name,
    COALESCE(s.region, '')       AS region,
    COALESCE(s.municipality, '') AS municipality,
    v.last_verified,
    v.last_verified_by,
    v.events_through,
    (v.last_verified IS NOT NULL) AS is_verified,
    (v.events_through IS NULL OR EXISTS (
        SELECT 1 FROM fact_event e
        WHERE e.site_key = s.site_key
          AND e.event_time > v.events_through
    )) AS has_unverified_events
FROM dim_site s
LEFT JOIN v_site_verification_status v USING (site_key);

-- 5. report
SELECT
  (SELECT count(*) FROM dim_user)                AS users,
  (SELECT count(*) FROM dim_region_canonical)    AS regions,
  (SELECT count(*) FROM v_verified_inventory
     WHERE is_verified)                          AS verified_sites,
  (SELECT count(*) FROM v_verified_inventory
     WHERE NOT is_verified)                      AS unverified_sites;

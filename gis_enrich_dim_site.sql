-- ============================================================
-- GIS → dim_site enrichment
-- Pass 1: exact NENAME match  (plain BTS keys: CATRNJA, ADA…)
-- Pass 2: trgm fuzzy match    (prefixed keys: FTTE_KARAULA…)
-- Safe: only fills NULL cells, never overwrites existing data
-- Requires: gis_sites table + pg_trgm already loaded
-- ============================================================

-- Pass 1 — exact
UPDATE dim_site ds
SET
    latitude     = gs.lat,
    longitude    = gs.lon,
    region       = COALESCE(ds.region,       gs.region_code),
    municipality = COALESCE(ds.municipality, gs.lokacija),
    technologies = COALESCE(ds.technologies, string_to_array(gs.tehnologija, '/'))
FROM gis_sites gs
WHERE ds.site_key = gs.nename
  AND (ds.latitude IS NULL OR ds.longitude IS NULL);

-- Pass 2 — fuzzy (strips common prefixes + suffixes before matching)
UPDATE dim_site ds
SET
    latitude     = gs.lat,
    longitude    = gs.lon,
    region       = COALESCE(ds.region,       gs.region_code),
    municipality = COALESCE(ds.municipality, gs.lokacija),
    technologies = COALESCE(ds.technologies, string_to_array(gs.tehnologija, '/'))
FROM LATERAL (
    SELECT lat, lon, region_code, lokacija, tehnologija, nename,
           similarity(
               nename,
               regexp_replace(ds.site_key,
                   '^(FTTE|US|TKC|OSS|BS)_|_(MIKRO|RR|RR_PRENOS|KWP\d*|CORE|INDOOR)$',
                   '', 'gi')
           ) AS sim
    FROM gis_sites
    ORDER BY sim DESC
    LIMIT 1
) gs
WHERE ds.latitude IS NULL
  AND ds.longitude IS NULL
  AND gs.sim > 0.40;

-- Inspect results
SELECT
    COUNT(*)                                        AS total_sites,
    COUNT(latitude)                                 AS have_coords,
    COUNT(*) FILTER (WHERE latitude IS NULL)        AS still_missing,
    ROUND(COUNT(latitude)::numeric / COUNT(*) * 100, 1) AS pct_matched
FROM dim_site;

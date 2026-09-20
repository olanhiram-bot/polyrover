CREATE TABLE IF NOT EXISTS polyrover_predictions (
    id BIGSERIAL PRIMARY KEY,
    slug TEXT NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL,
    research_valid_until TIMESTAMPTZ NOT NULL,
    report JSONB NOT NULL,
    stored_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (slug, generated_at),
    CHECK (research_valid_until = generated_at + INTERVAL '24 hours')
);
CREATE INDEX IF NOT EXISTS polyrover_predictions_latest
    ON polyrover_predictions (slug, generated_at DESC);

CREATE TABLE IF NOT EXISTS polyrover_research_jobs (
    id BIGSERIAL PRIMARY KEY,
    slug TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    retry_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'interrupted')),
    finished_at TIMESTAMPTZ,
    UNIQUE (slug, started_at)
);
CREATE INDEX IF NOT EXISTS polyrover_jobs_latest
    ON polyrover_research_jobs (slug, started_at DESC);
CREATE INDEX IF NOT EXISTS polyrover_jobs_running
    ON polyrover_research_jobs (lease_until) WHERE status = 'running';

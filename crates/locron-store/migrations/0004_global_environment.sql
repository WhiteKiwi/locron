ALTER TABLE settings
ADD COLUMN environment_json TEXT NOT NULL DEFAULT '{}';

-- A human-readable display name for a user's profile (optional; defaults blank).
ALTER TABLE users ADD COLUMN display_name TEXT NOT NULL DEFAULT '';

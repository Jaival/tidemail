-- The re-sanitize job looks for bodies stored by an older sanitizer.
CREATE INDEX bodies_sanitizer_version ON bodies(sanitizer_version);

-- Snippets synced before Phase 2's fix were stored HTML-escaped, as Gmail
-- sends them. Decode the common entities; `&amp;` last.
UPDATE messages SET snippet =
  replace(replace(replace(replace(replace(replace(snippet,
    '&#39;', ''''), '&quot;', '"'), '&lt;', '<'), '&gt;', '>'), '&nbsp;', ' '), '&amp;', '&')
WHERE snippet LIKE '%&%;%';
UPDATE threads SET snippet =
  replace(replace(replace(replace(replace(replace(snippet,
    '&#39;', ''''), '&quot;', '"'), '&lt;', '<'), '&gt;', '>'), '&nbsp;', ' '), '&amp;', '&')
WHERE snippet LIKE '%&%;%';

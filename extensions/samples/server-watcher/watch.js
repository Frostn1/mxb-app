// Shared by the background page and the tab, which both load it with <script src>; the app
// inlines it into each page's sandbox. What counts as a watched server.

/** Comma-separated words from the setting, lower-cased. Empty means "any track". */
function watchWords(setting) {
  return String(setting || "")
    .toLowerCase()
    .split(",")
    .map((w) => w.trim())
    .filter(Boolean);
}

/** A server is watched when its track (or name) contains any of the words. */
function isWatched(server, words) {
  if (words.length === 0) return true;
  const hay = `${server.track} ${server.name}`.toLowerCase();
  return words.some((w) => hay.includes(w));
}

// Shared by the tab and the background page (loaded with <script src>, inlined by the app).

const TYPE_LABEL = { tracks: "Tracks", bikes: "Bikes", rider: "Rider gear", paints: "Paints", tyres: "Tyres", sounds: "Sounds" };

function typeLabel(t) {
  return TYPE_LABEL[t] || t.charAt(0).toUpperCase() + t.slice(1);
}

function bytes(n) {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(0)} MB`;
  return `${Math.max(1, Math.round(n / 1024))} KB`;
}

/** Totals per type, largest first, and the biggest mods overall. */
function summarise(mods) {
  const byType = new Map();
  for (const m of mods) {
    const e = byType.get(m.type) || { type: m.type, count: 0, size: 0 };
    e.count += 1;
    e.size += m.size;
    byType.set(m.type, e);
  }
  const types = [...byType.values()].sort((a, b) => b.size - a.size);
  const biggest = [...mods].sort((a, b) => b.size - a.size).slice(0, 15);
  const total = types.reduce((s, t) => s + t.size, 0);
  return { types, biggest, total, count: mods.length };
}

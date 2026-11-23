// Coach Laps: MXB Coach's sessions through `mxb.coach.sessions()` (needs `coach.read`).

const $ = (id) => document.getElementById(id);

function fmt(ms) {
  if (ms == null) return "–";
  const m = Math.floor(ms / 60000);
  const s = ((ms % 60000) / 1000).toFixed(3).padStart(6, "0");
  return `${m}:${s}`;
}
const avg = (xs) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);
function spread(xs) {
  if (xs.length < 2) return null;
  const a = avg(xs);
  return Math.sqrt(xs.reduce((s, x) => s + (x - a) ** 2, 0) / xs.length);
}
function when(started) {
  const m = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})/.exec(started);
  if (!m) return started;
  return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5]).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
const TYPE = { testing: "Testing", race: "Race", "straight-rhythm": "Straight rhythm", other: "Other" };

function cell(text, cls) {
  const td = document.createElement("td");
  td.textContent = text; // other people's track and bike names: text, never HTML
  if (cls) td.className = cls;
  return td;
}
function row(...cells) {
  const tr = document.createElement("tr");
  tr.append(...cells);
  return tr;
}

async function render() {
  let sessions;
  try {
    sessions = await mxb.coach.sessions();
  } catch (e) {
    $("empty").hidden = false;
    $("empty").textContent = String(e.message || e).includes("permission")
      ? "Reading MXB Coach sessions isn't allowed. Turn it on in Extensions → Installed."
      : `Couldn't read MXB Coach: ${e.message || e}`;
    return;
  }
  const { sessions: limit } = await mxb.settings.get();
  const withLaps = sessions.filter((s) => s.cleanLaps.length > 0);
  $("sub").textContent = `${sessions.length} sessions recorded`;
  if (withLaps.length === 0) {
    $("empty").hidden = false;
    $("empty").textContent = "No clean laps yet. Ride with MXB Coach recording and they show up here.";
    return;
  }

  const last = withLaps[0];
  $("last").hidden = false;
  $("lastTitle").textContent = last.track;
  $("lastMeta").textContent = `${last.bike} · ${last.category} · ${TYPE[last.event]} · ${when(last.started)}`;
  $("lastBest").textContent = fmt(Math.min(...last.cleanLaps));
  $("lastAvg").textContent = fmt(avg(last.cleanLaps));
  $("lastClean").textContent = `${last.cleanLaps.length} / ${last.laps}`;
  const laps = last.cleanLaps.slice(-30);
  const lo = Math.min(...laps);
  const hi = Math.max(...laps);
  $("bars").replaceChildren(
    ...laps.map((t) => {
      const b = document.createElement("div");
      // Faster is taller: the bar shows pace, not time.
      const h = hi === lo ? 1 : 1 - (t - lo) / (hi - lo);
      b.className = t === lo ? "bar best" : "bar";
      b.style.height = `${20 + h * 80}%`;
      b.title = fmt(t);
      return b;
    }),
  );

  const byTrack = new Map();
  for (const s of withLaps) {
    const k = `${s.track}\u0000${s.bike}`;
    const e = byTrack.get(k) ?? { track: s.track, bike: s.bike, clean: [] };
    e.clean.push(...s.cleanLaps);
    byTrack.set(k, e);
  }
  const pbs = [...byTrack.values()]
    .map((e) => ({ ...e, best: Math.min(...e.clean) }))
    .sort((a, b) => b.clean.length - a.clean.length);
  $("pbs").replaceChildren(
    ...pbs.slice(0, 12).map((e) => {
      const sd = spread(e.clean);
      return row(
        cell(e.track),
        cell(e.bike),
        cell(fmt(e.best), "num"),
        cell(fmt(avg(e.clean)), "num"),
        cell(sd == null ? "–" : `±${(sd / 1000).toFixed(2)}s`, "num"),
        cell(String(e.clean.length), "num"),
      );
    }),
  );

  // Sessions with no lap at all (a menu, a crash on the first corner) aren't worth a row.
  $("recent").replaceChildren(
    ...sessions
      .filter((s) => s.laps > 0)
      .slice(0, Number(limit) || 10)
      .map((s) =>
        row(
          cell(when(s.started)),
          cell(s.track),
          cell(s.bike),
          cell(TYPE[s.event]),
          cell(fmt(s.bestMs), "num"),
          cell(String(s.laps), "num"),
        ),
      ),
  );
}

mxb.settings.onChange(render);
render();

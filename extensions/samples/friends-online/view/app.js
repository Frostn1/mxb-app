// Friends Online: everything here talks to the app through `window.mxb`, the only API a view
// has. It runs in a sandboxed frame: no Tauri, no Node, no file system, no network.

const list = document.getElementById("list");
const statusEl = document.getElementById("status");
const gameEl = document.getElementById("game");
let timer = 0;

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  // textContent, never innerHTML: names come from other players.
  if (text !== undefined) e.textContent = text;
  return e;
}

function ago(ms) {
  const m = Math.round((Date.now() - ms) / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  return h < 48 ? `${h} h ago` : `${Math.round(h / 24)} d ago`;
}

async function render() {
  const settings = await mxb.settings.get();
  let friends;
  try {
    friends = await mxb.friends.list();
  } catch (e) {
    statusEl.textContent = String(e.message || e).includes("permission")
      ? "Friends presence isn't allowed. Turn it on in Extensions → Installed."
      : `Couldn't load friends: ${e.message || e}`;
    list.replaceChildren();
    return;
  }
  const online = friends.filter((f) => f.online);
  const shown = settings.showOffline ? friends : online;
  statusEl.textContent = friends.length === 0
    ? "No friends yet. Add some from the Online tab."
    : `${online.length} of ${friends.length} online`;
  list.replaceChildren(
    ...shown
      .sort((a, b) => Number(b.online) - Number(a.online) || a.name.localeCompare(b.name))
      .map((f) => {
        const li = el("li");
        li.append(el("span", f.online ? "dot on" : "dot"));
        const who = el("div");
        who.append(el("div", "name", f.name));
        const where = f.server
          ? `${f.server}${f.track ? ` · ${f.track}` : ""}`
          : f.online ? "Online" : f.lastSeenAt ? `Seen ${ago(f.lastSeenAt)}` : "Offline";
        who.append(el("div", "where", where));
        li.append(who);
        li.append(el("span", "riders", f.riders != null ? `${f.riders} riding` : ""));
        return li;
      }),
  );
}

function schedule(settings) {
  clearInterval(timer);
  const secs = Number(settings.refreshSeconds) || 30;
  timer = setInterval(render, secs * 1000);
}

document.getElementById("refresh").addEventListener("click", render);
mxb.settings.onChange((s) => {
  schedule(s);
  render();
});

mxb.game
  .state()
  .then((g) => {
    const show = (s) => {
      gameEl.textContent = s.running ? "In game" : "Game closed";
      gameEl.className = s.running ? "pill on" : "pill";
    };
    show(g);
    mxb.game.onChange(show);
  })
  .catch(() => gameEl.remove());

mxb.settings.get().then(schedule);
render();

// `window.mxb` is the whole API. See docs/extensions.md for every call.

async function hello() {
  const { greeting } = await mxb.settings.get();
  await mxb.ui.toast(`${greeting} from My Extension`);
}

document.getElementById("hi").addEventListener("click", hello);
mxb.commands.on("hello", hello);

mxb.game
  .state()
  .then((s) => {
    const show = (g) => (document.getElementById("game").textContent = g.running ? "MX Bikes is running" : "MX Bikes is closed");
    show(s);
    mxb.game.onChange(show);
  })
  .catch((e) => (document.getElementById("game").textContent = e.message));

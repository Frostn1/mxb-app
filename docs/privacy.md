# Privacy

This is what MXB App, MXB Studio, MXB Coach and FrostMod send, what mxbsecure holds, how long
any of it stays, and how to get rid of it.

It is written to be checkable. Every claim here matches something in the code, and the parts
that would be convenient to leave out are in here too.

**Who holds it:** Creste LLC, a software company in the United States.
**How to reach us:** hello@creste.dev, or the Discord at https://discord.gg/3994Rr3ywb.

## The short version

- The apps do not ask for your name, your email or your address, and there is nowhere to type
  one.
- Your account is a random id, the rider name you already use in game, and your MX Bikes GUID.
- We never store your IP address. Where an IP has to be counted, a salted digest of it is
  counted instead, and the salt changes daily.
- Crash dumps and log files never leave your PC unless you press the button that sends them.
- Anonymous usage counters can be turned off in Settings, General.
- Everything held about you can be deleted on request, in one action, and we will tell you
  exactly what went and what was kept.

## What the apps send

**Your account.** A random account id, your rider name, your MX Bikes GUID, and your Steam id
once you sign in with Steam. The sign-in token is stored only as a SHA-256 digest, so a copy
of the database holds nothing anyone could present as your credential. The GUID and the Steam
id are what make a purchase open on your machine and what make a ban stick to a person rather
than to a reinstall.

**Paint sync**, if you switch it on. The paint files you publish, their names, their sizes and
their checksums, so the riders beside you see your bike the way you built it. Image data is
stored by its checksum, so two riders with the same paint share one copy.

**Where you are playing**, while you are playing. The server your app says you are on, so the
Servers tab can show who is there and so voice can find the room. It is replaced by the next
one and is not a history.

**Voice**, if you use it. No audio reaches us. The room passes the few kilobytes of connection
details two apps need to find each other, and the voice itself goes straight between the two
PCs. What is held while a room is open is your account id, your rider name and your race
number.

**Anonymous usage counters**, unless you turn them off. A random install id that is tied to no
account, the app version, the operating system, which game is installed, and counters like
"the Servers tab was opened". The names that can be counted are a fixed list compiled into the
app, which is what makes it impossible for a path, a rider name or a mod title to end up in
one. The same applies to the one-tap surveys the app sometimes shows.

**Crash reports**, when MX Bikes closes on its own. The faulting module and offset, the call
stack as addresses, the game build, the FrostMod and app versions, the track and server you
were on, how many riders were in the session, how long the game had been running, and your
rider name and GUID. This is how a crash that happens to hundreds of people gets found and
fixed, which is work we do for free and do not charge anyone for.

**What is loaded inside your game**, while it is running. The file names of the modules loaded
into MX Bikes, where each one came from, a checksum for anything that is not a Windows system
file, and what each file says about itself: its size, its date, whether Windows trusts its
signature, and the company it claims. Never the file paths. A path carries your Windows user
folder, so only the file name is ever taken. This exists to find injected cheats and the
broken overlays that crash the game, and to keep locked content locked. We look at what is in
the game, not at what is on your PC.

**Purchases**, if you buy protected content. Your Steam id against the thing you bought, so it
opens on your machine, and your Steam id on the seller's buyer list, so they can see who has
it and add or remove people.

**Logs, only when you send them.** The Send logs button zips the app's own logs and uploads
them to a public file host, which the app tells you before it does. Logs hold folder paths and
what the app was doing. They do not hold passwords or session cookies, and no settings file is
included. The upload expires on its own after a few days.

## What never leaves your PC

- **Crash dumps.** The dump beside a crash report is a copy of the game's memory. It stays on
  your machine unless you press Send it.
- **Your IP address.** Nothing stores one. Where requests have to be counted per machine, a
  daily salted digest is counted, and those counters are deleted the next day.
- **File paths.** Neither the module reports nor the usage counters can carry one.
- **Coach recordings.** Laps recorded by MXB Coach are files on your disk and are not uploaded.
- **Payment details.** Buy Me a Coffee and Steam handle payment. We never see a card.
- **Your locked file, if you are a creator.** Locking runs in your browser. Only the title, the
  buyer list and a fingerprint of the locked file reach the server.

## Why we are allowed to hold it

- Your account, paint sync, presence, voice and purchases: because you asked for the feature
  and it cannot work without them. In GDPR terms, performing the contract you are in when you
  use the app.
- Crash reports, module reports, bans and the diagnostics behind them: legitimate interests.
  Specifically, keeping the game from crashing for everyone it crashes on, keeping cheats out
  of the lobbies people ride in, and keeping bought content from being stripped and handed
  around. We have weighed that against what it costs you, which is why the reports carry file
  names rather than paths and why the names on a crash are wiped after a month.
- Anonymous usage counters: consent, which is why the switch is in Settings and why turning it
  off stops the reports and clears the install id.

## How long we keep it

| What | How long |
| --- | --- |
| Account, GUID, Steam link | Until you delete it |
| Published paints, presence, queue | Until you delete it, or until you unpublish |
| Module reports | 90 days |
| Crash reports | Rider name and GUID wiped after 30 days, the row deleted after a year |
| Usage counters and surveys | 400 days |
| Per-day request counters (the IP digests) | 3 days |
| Server addresses and who reported seeing them | 30 days |
| Purchases | While the content exists, because they are what makes it open |
| Bans, and the GUID claims that prove one is yours | Indefinitely, see below |

## Who else sees it

- **Cloudflare**, who run the servers and the database, as our processor.
- **Amazon Web Services**, when a race server is provisioned, because that is where it runs.
- **Steam**, when you sign in, which tells us your Steam id and your display name.
- **Buy Me a Coffee**, if you buy a coffee. Supporters are listed in the app by name and with
  permission, and a word on Discord takes yours off.
- **The seller**, if you buy protected content. They see your Steam id on their buyer list.
- **The file host**, for the few days a log zip you sent is up.

Nothing here is sold, and nothing here goes to an advertiser. There are no ad networks and no
third-party analytics in any of the apps.

## Your choices

- **Turn off the usage counters.** Settings, General. Nothing further is sent, and anything
  counted but not yet sent is dropped rather than flushed on the way out. The install id stays
  in your own settings file, where it is the only place it has ever been kept.
- **Turn off paint sync**, and unpublish what you published. Settings, Paint sync.
- **Do not send a crash dump.** Dismiss the prompt, and the dump stays where it is.
- **Delete everything.** Ask on Discord or by email and it is done the same way you could do
  it yourself: one call, `DELETE /v1/me`, authenticated by your own app's token. It empties
  every table that describes you, clears your rider name, Steam id and GUID off the account,
  and destroys the token, so the account cannot be used again. The answer lists what went and
  what was kept.
- **Ask for the rest of it**, at hello@creste.dev. A copy of what is held, a correction to
  something that is wrong, or an objection to the crash and module reports described above. If you are in the EU or the
  UK you can also complain to your data protection authority.

Two things survive a deletion, and they are named in the answer it gives you:

1. **A ban, and the GUID claims that link it to you.** Otherwise asking to be forgotten would
   be the cheapest way to get around a ban, and the ban exists to protect the people who ride
   with you. The regulation allows keeping what is needed for overriding legitimate interests,
   and this is that.
2. **Purchases.** They are keyed to your Steam id and they are what keeps content you paid for
   working, and what keeps a creator's buyer list honest. Deleting them would take something
   off you, not give something back.

Everything else goes, including the crash reports, the module reports, the paints and the
presence, banned or not.

## Where it is held

Creste LLC is in the United States, and the servers are Cloudflare's and AWS's, which run
worldwide. If you are in the EU or the UK, that means your data is processed outside your
country, including in the United States, under the transfer terms in Cloudflare's and AWS's own
data processing agreements. We have not appointed a representative in the EU. Write to
hello@creste.dev and you are talking to the person who holds the data, not to a desk in
between.

## Age

There is no age check in the apps and we do not ask for a birthday. The apps are not aimed at
children. If a parent or guardian wants an account and everything on it removed, the deletion
above does it, and asking on Discord or by email works as well.

## Changes

Material changes are noted in the release notes of the version that carries them. The date at
the bottom is when this text last changed.

Last updated: 2026-09-17.

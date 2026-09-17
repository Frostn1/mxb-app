# Crash reports: from the game to the dashboard

MX Bikes closes to desktop on its own. Landing an overjump, hitting an object, clicking go to
track, and worse on a busy server. It predates MXB App and FrostMod, and it lands on people
who are running our app when it happens.

Until this existed, the whole of the evidence was one line in one player's log. That was
enough to take apart exactly one crash (`mxbikes.exe+0x11D753`, a null deref in the game's
GHS handle pool, guarded in FrostMod v0.28.0) and it took a disassembler to do it, because
one address was all there was. The reason there was only one is that nothing collected the
others.

## The path a crash takes

    FrostMod, inside the game        frostmod-crash-<stamp>.json   beside its log
                                     frostmod-crash-<stamp>.dmp    beside that

    MXB App, crashreports.rs         PUT /v1/diagnostics/crash     the JSON only
                                     the dump stays on the machine

    control plane, crashes.ts        client_crashes                one row per crash

    admin                            /v1/web/admin/crashes         ranked by riders hit

## Three properties that are the design, not an accident

**A file, not a socket.** At the moment of the crash the game has seconds at best, the
network stack is the last thing to trust, and the app may not be running at all. A file
survives all three, and a crash that happened with the app closed still arrives. The app
looks at startup and at the end of every session.

**The dump never leaves on its own.** It is megabytes of process memory. The app asks once
per crash, through the runtime banner, and records that it asked whichever way the player
answered. The JSON beside it is addresses and short strings, and goes with the diagnostics
the app already sends.

**Ranked by distinct riders, not by hits.** A hundred crashes from one machine is one person
having a bad night; three across three accounts is a bug. The ranking decides what gets a
week of someone's time, so it has to be the honest number.

## What a report holds

`site` is the key everything groups by: `module+0xRVA` for the faulting instruction, which is
the one thing two players hitting the same bug will both send. Around it: the exception kind
and code, the address it was refused at, the call stack, which build of the game and of
FrostMod, where the player was (on track, spectating, in the menus), the track and server,
how many riders were in the session, how long since the last frame and the last content
reload, and the last thirty two things FrostMod noted before it went.

Absent stays absent. `riders: null` is not in a race; `riders: 0` would be a grid of nobody.

## Sending it again

The app renames a sent report to `.sent`. A send whose answer was lost on the way back is
retried on the next look, and the control plane's unique index over (account, crash time,
site) means the retry is not a second row. A crash counted twice is a crash that looks twice
as common as it is.

## Where the code is

| what | where |
| --- | --- |
| the report, and the JSON it is written as | `frostmod/src/crashreport.h`, `crashreport.cpp` |
| finding and sending them | `apps/manager/src-tauri/src/crashreports.rs` |
| when it looks | `apps/manager/src-tauri/src/sessionwatch.rs` |
| the banner | `apps/manager/src/components/RuntimeBanner/RuntimeBanner.tsx` |
| taking them in, and the reads | `control-plane/src/crashes.ts` |
| the table | `control-plane/migrations/0038_crash_reports.sql` |

The dashboard page that renders the reads lives in `mxbsecure-web` and is not built yet.

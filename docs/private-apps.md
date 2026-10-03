# Private apps

Source for MXB Coach, Frost's Studio and MXB Servers lives in the private Frostn1/mxb-apps-private, not here.
The release workflows stay here (the signing secrets live on this repo): each release job checks the private
repo out with the read-only deploy key secret `APPS_PRIVATE_DEPLOY_KEY`, then runs its `apply.sh`, which copies
`apps/{coach,studio,servers}` in, restores the three workspace members and swaps in the lockfiles from the split.
Release logs are public: `apply.sh` is quiet and the build sets `RUSTFLAGS=--cap-lints allow`, but a compile
error can still quote source, so re-run failed private releases only after checking the log.
Shared crates/packages (`crates/*`, `packages/*`) stay here.

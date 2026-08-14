//! Getting a rider through a server's gate, and putting their setup back afterwards.
//!
//! A server turns riders away for reasons the game never explains — it refuses the bike and
//! the player is simply not on the grid. There are three of those reasons and this module
//! knows all three, so the answer arrives before the launch rather than after it:
//!
//!   1. **Class.** `[event] category` is a `/`-separated spec, empty meaning Open, and a
//!      bike passes by its own `[data] cat`. Matching is [`crate::bikeswap::class_matches`],
//!      which already mirrors the server's rule for the in-garage swap.
//!   2. **Bike id.** A server may name the exact bikes it takes (`allowed_bikes`).
//!   3. **What the host has.** A dedicated server refuses any bike it does not itself have
//!      installed, whatever its config allows.
//!
//! Being eligible is usually one change away: ride the bike the server does take. That
//! change is not one anybody wants to keep, which is the other half of this module.
//! Everything a join overwrites is snapshotted first, so "switch bikes to get in" is
//! temporary and undoable rather than an edit to the player's profile they have to remember
//! to reverse.

use crate::bikeswap::class_matches;
use serde::{Deserialize, Serialize};

/// Why a bike will or will not get a rider onto a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    Allowed,
    /// The bike's class is not one this server's category spec admits.
    WrongClass,
    /// The server names the bikes it takes, and this is not one of them.
    NotAllowedBike,
    /// A mod bike the host has not got installed, so it is refused at the gate whatever
    /// the config says.
    MissingOnServer,
    /// Not enough known about the server to say. Never treated as a refusal — a browser
    /// that blocks joins whenever it could not reach something is worse than one that lets
    /// the player try.
    Unknown,
}

impl Verdict {
    /// Whether this verdict should stop a join. Only an outright refusal does; `Unknown` is
    /// ignorance, not a closed door.
    pub fn blocks(self) -> bool {
        !matches!(self, Verdict::Allowed | Verdict::Unknown)
    }
}

/// What a server will let through, from whichever sources could be asked.
///
/// Every field is separately optional because the sources are separate: the public list
/// carries the category spec, while the bikes a host has installed are only knowable by
/// asking that host's agent. A gate that knew one and not the other still has to give a
/// useful answer rather than falling back to "no idea".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Gate {
    /// `[event] category`. Empty is Open, and is a real answer rather than a missing one.
    pub category: String,
    /// The ids the server names, when it names any. Empty means no id restriction.
    pub allowed_bikes: Vec<String>,
    /// What the host has under `mods/bikes`, when an agent could be asked. `None` is "we
    /// could not ask", which is not the same as an empty list — a stock install has no such
    /// folder and still accepts every stock bike.
    pub host_bikes: Option<Vec<String>>,
    /// Whether anything here was actually answered. False means the whole gate is a guess
    /// and every verdict is [`Verdict::Unknown`].
    pub known: bool,
}

fn has(list: &[String], want: &str) -> bool {
    list.iter().any(|b| b.eq_ignore_ascii_case(want))
}

/// Whether this bike gets this rider in.
///
/// `local_mod_bikes` is `mods/bikes` on *this* machine, and it is what makes the third check
/// possible: a bike missing from the host's list is only a problem if it is a mod, since
/// stock bikes live inside the game's locked archive and appear in neither machine's folder
/// while both installs certainly have them. Without that distinction every stock bike would
/// read as refused on every server.
pub fn verdict(gate: &Gate, local_mod_bikes: &[String], bike_id: &str, bike_class: &str) -> Verdict {
    let bike = bike_id.trim();
    if bike.is_empty() || !gate.known {
        return Verdict::Unknown;
    }
    if !class_matches(bike_class, &gate.category) {
        return Verdict::WrongClass;
    }
    if !gate.allowed_bikes.is_empty() && !has(&gate.allowed_bikes, bike) {
        return Verdict::NotAllowedBike;
    }
    if let Some(host) = gate.host_bikes.as_deref() {
        if !has(host, bike) && has(local_mod_bikes, bike) {
            return Verdict::MissingOnServer;
        }
    }
    Verdict::Allowed
}

/// A bike the rider owns, as the gate needs to see it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    #[serde(default)]
    pub class: String,
}

/// The rider's own bikes this server would take, in the order they were given.
///
/// What the UI offers when the chosen bike is refused: a refusal with a way out rather than
/// a dead end. An empty result means switching bikes cannot help, which is worth saying
/// plainly instead of showing an empty picker.
pub fn eligible_bikes(
    gate: &Gate,
    local_mod_bikes: &[String],
    candidates: &[Candidate],
) -> Vec<String> {
    candidates
        .iter()
        .filter(|c| !verdict(gate, local_mod_bikes, &c.id, &c.class).blocks())
        .map(|c| c.id.clone())
        .collect()
}

/// What a join overwrote, and everything needed to put it back.
///
/// Persisted in the app config rather than held in memory: the whole point is that the
/// player rides, quits, and comes back — possibly after closing the app, possibly days
/// later — and still finds their own setup one click away.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PendingRestore {
    pub profile: String,
    /// The bike the preset was written onto — the column in `profile.ini` to restore.
    pub bikeid: String,
    /// What that column held before.
    pub loadout: crate::presets::Loadout,
    /// `[info] bikeid` before the join, so a switch to another bike is undone as well as
    /// the cosmetics written onto it.
    pub active_bike: String,
    /// Which server this was for, so the offer to undo can name it.
    pub server: String,
    /// The preset that was applied, for the same reason.
    pub preset: String,
    /// Unix seconds. The UI ages the offer rather than showing a restore with no idea how
    /// old it is.
    pub at: i64,
}

impl PendingRestore {
    pub fn is_set(&self) -> bool {
        !self.profile.trim().is_empty() && !self.bikeid.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// An open server that answered — no category, no id list, no agent.
    fn open() -> Gate {
        Gate { known: true, ..Default::default() }
    }

    #[test]
    fn an_open_server_takes_anything() {
        assert_eq!(verdict(&open(), &strings(&["mod450"]), "mod450", "MX1"), Verdict::Allowed);
    }

    #[test]
    fn a_bike_outside_the_category_is_refused() {
        let gate = Gate { category: "MX2".into(), ..open() };
        assert_eq!(verdict(&gate, &[], "kx450", "MX1"), Verdict::WrongClass);
        assert_eq!(verdict(&gate, &[], "kx250", "MX2"), Verdict::Allowed);
    }

    #[test]
    fn any_member_of_a_multi_category_spec_passes() {
        // The server's own rule: `/`-separated, matching any member is enough.
        let gate = Gate { category: "MX1/MX2".into(), ..open() };
        assert_eq!(verdict(&gate, &[], "kx450", "MX1"), Verdict::Allowed);
        assert_eq!(verdict(&gate, &[], "kx250", "MX2"), Verdict::Allowed);
        assert_eq!(verdict(&gate, &[], "sm", "Supermoto"), Verdict::WrongClass);
    }

    #[test]
    fn an_unclassified_bike_cannot_enter_a_restricted_race() {
        // Mirrors the server: a bike declaring no class satisfies no spec but Open.
        let gate = Gate { category: "MX1".into(), ..open() };
        assert_eq!(verdict(&gate, &[], "someMod", ""), Verdict::WrongClass);
        assert_eq!(verdict(&open(), &[], "someMod", ""), Verdict::Allowed);
    }

    #[test]
    fn a_server_that_names_its_bikes_refuses_the_others() {
        let gate = Gate { allowed_bikes: strings(&["kx450"]), ..open() };
        assert_eq!(verdict(&gate, &[], "kx450", "MX1"), Verdict::Allowed);
        assert_eq!(verdict(&gate, &[], "crf450", "MX1"), Verdict::NotAllowedBike);
    }

    #[test]
    fn matches_regardless_of_how_the_two_sides_spelled_it() {
        // A host's folder name and the id in `profile.ini` agree by convention only, and an
        // exact compare would refuse a bike both machines have.
        let gate = Gate { allowed_bikes: strings(&["KX450"]), ..open() };
        assert_eq!(verdict(&gate, &[], "kx450", "MX1"), Verdict::Allowed);
    }

    #[test]
    fn a_mod_bike_the_host_lacks_is_refused_even_when_the_config_allows_it() {
        // The failure with no explanation in-game: the config says yes, the box hasn't got
        // the files, and the rider is turned away.
        let gate = Gate { host_bikes: Some(strings(&["kx450"])), ..open() };
        assert_eq!(
            verdict(&gate, &strings(&["crf450-mod"]), "crf450-mod", "MX1"),
            Verdict::MissingOnServer
        );
    }

    #[test]
    fn a_stock_bike_is_allowed_even_though_it_is_on_neither_machines_list() {
        // Stock bikes live inside the game's locked archive, so they are in no `mods/bikes`
        // folder on either side — and both installs have them.
        let gate = Gate { host_bikes: Some(strings(&["kx450"])), ..open() };
        assert_eq!(verdict(&gate, &strings(&["crf450-mod"]), "stock_250", "MX2"), Verdict::Allowed);
    }

    #[test]
    fn a_host_with_no_mod_bikes_still_accepts_stock_ones() {
        // An empty list is a real answer — a stock install has no `mods/bikes` folder at
        // all — and reading it as "accepts nothing" would refuse every join.
        let gate = Gate { host_bikes: Some(Vec::new()), ..open() };
        assert_eq!(verdict(&gate, &strings(&["crf450-mod"]), "stock_450", "MX1"), Verdict::Allowed);
        assert_eq!(
            verdict(&gate, &strings(&["crf450-mod"]), "crf450-mod", "MX1"),
            Verdict::MissingOnServer
        );
    }

    #[test]
    fn a_server_we_could_not_ask_never_refuses_anyone() {
        // An unreachable or unlisted server must not become a locked door. Better to let
        // the player try and be turned away by the game than to block on our own ignorance.
        let gate = Gate { category: "MX2".into(), known: false, ..Default::default() };
        assert_eq!(verdict(&gate, &[], "kx450", "MX1"), Verdict::Unknown);
    }

    #[test]
    fn not_knowing_what_the_host_installed_is_not_a_refusal() {
        // `host_bikes: None` is the normal case for a public server: nothing to ask.
        let gate = Gate { category: "MX1".into(), ..open() };
        assert_eq!(verdict(&gate, &strings(&["crf450-mod"]), "crf450-mod", "MX1"), Verdict::Allowed);
    }

    #[test]
    fn no_bike_chosen_is_unknown_rather_than_allowed() {
        assert_eq!(verdict(&open(), &[], "  ", "MX1"), Verdict::Unknown);
    }

    #[test]
    fn only_a_real_refusal_blocks_a_join() {
        assert!(!Verdict::Allowed.blocks());
        assert!(!Verdict::Unknown.blocks());
        assert!(Verdict::WrongClass.blocks());
        assert!(Verdict::NotAllowedBike.blocks());
        assert!(Verdict::MissingOnServer.blocks());
    }

    #[test]
    fn offers_the_riders_own_bikes_that_would_get_them_in() {
        let gate = Gate { category: "MX2".into(), ..open() };
        let mine = [
            Candidate { id: "kx450".into(), class: "MX1".into() },
            Candidate { id: "kx250".into(), class: "MX2".into() },
            Candidate { id: "yz250".into(), class: "MX2".into() },
        ];
        assert_eq!(eligible_bikes(&gate, &[], &mine), strings(&["kx250", "yz250"]));
    }

    #[test]
    fn offers_everything_when_the_server_could_not_be_asked() {
        let gate = Gate { known: false, ..Default::default() };
        let mine = [
            Candidate { id: "kx450".into(), class: "MX1".into() },
            Candidate { id: "kx250".into(), class: "MX2".into() },
        ];
        assert_eq!(eligible_bikes(&gate, &[], &mine), strings(&["kx450", "kx250"]));
    }

    #[test]
    fn says_so_plainly_when_no_bike_can_get_in() {
        let gate = Gate { category: "Supermoto".into(), ..open() };
        let mine = [Candidate { id: "kx450".into(), class: "MX1".into() }];
        assert!(eligible_bikes(&gate, &[], &mine).is_empty());
    }

    #[test]
    fn a_restore_needs_both_a_profile_and_a_bike_to_mean_anything() {
        assert!(!PendingRestore::default().is_set());
        let half = PendingRestore { profile: "Frost".into(), ..Default::default() };
        assert!(!half.is_set());
        let whole =
            PendingRestore { profile: "Frost".into(), bikeid: "KX450".into(), ..Default::default() };
        assert!(whole.is_set());
    }
}

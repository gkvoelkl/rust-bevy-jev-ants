//! The texts put to Jev, as an asset rather than as code.
//!
//! `ANTS.md` §4.5 and rule 6: `instructions` lives in `assets/questions.ron` and
//! can be reworded while the game runs. That is not a convenience — rewording is
//! the actual development work in a game like this, and a rebuild between two
//! attempts is enough friction to stop anyone from doing it properly.

use bevy::prelude::*;
use serde::Deserialize;

/// Where the texts are read from, relative to the working directory.
pub const QUESTIONS_PATH: &str = "assets/questions.ron";

/// The fallback, compiled in. Used when the file is missing or unreadable —
/// including a file that has been edited into something that will not parse,
/// which is a thing that happens while wording is being worked on.
pub const DEFAULT_STEP: &str = "Which single step should this ant take now? \
     Follow the ant queen's order whenever it applies.";

/// The file as it is on disk, compiled into the binary for the browser, which
/// has none. It is the same text either way — `the_web_build_ships_the_text_on
/// _disk` sees to that — so rewording the file changes both builds, and only
/// the native one picks it up without a rebuild.
#[cfg(any(target_arch = "wasm32", test))]
const BUILT_IN: &str = include_str!("../../assets/questions.ron");

/// The question texts in force right now.
#[derive(Resource, Clone, Deserialize)]
pub struct Questions {
    /// The one `choice` question of step 1. `urgency` and `follows_order` join
    /// it here in step 2, against the same state.
    pub step: String,
}

impl Default for Questions {
    fn default() -> Self {
        Self {
            step: DEFAULT_STEP.to_string(),
        }
    }
}

impl Questions {
    /// Reads the file, or says why it could not. The game never fails over this:
    /// a broken file means the compiled-in text and a line in the log, not a
    /// colony that will not start.
    pub fn load() -> Result<Self, String> {
        // No file system in the browser, so the copy that was compiled in
        // stands in for it. Rewording still works there — it happens in the
        // field in the game — it simply does not outlive the tab.
        #[cfg(target_arch = "wasm32")]
        let text = BUILT_IN.to_string();

        #[cfg(not(target_arch = "wasm32"))]
        let text = std::fs::read_to_string(QUESTIONS_PATH)
            .map_err(|error| format!("{QUESTIONS_PATH}: {error}"))?;

        let parsed: Questions =
            ron::from_str(&text).map_err(|error| format!("{QUESTIONS_PATH}: {error}"))?;
        if parsed.step.trim().is_empty() {
            return Err(format!("{QUESTIONS_PATH}: the step question is empty"));
        }
        Ok(parsed)
    }

    /// What the game starts with: the file when it reads, the compiled-in text
    /// otherwise, and either way a line saying which.
    pub fn load_or_default() -> Self {
        match Self::load() {
            Ok(questions) => {
                info!("question texts from {QUESTIONS_PATH}");
                questions
            }
            Err(reason) => {
                warn!("using the built-in question text — {reason}");
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file that ships with the game has to parse. If it does not, the game
    /// runs on the fallback and nobody notices that their edits do nothing.
    #[test]
    fn the_shipped_file_reads() {
        let questions = Questions::load().expect("assets/questions.ron parses");
        assert!(!questions.step.trim().is_empty());
    }

    /// The browser plays on the copy. If it has drifted from the file, the two
    /// builds ask Jev different questions and every measurement taken on one
    /// stops holding for the other.
    #[test]
    fn the_web_build_ships_the_text_on_disk() {
        let on_disk = std::fs::read_to_string(QUESTIONS_PATH).expect("the file is there");
        assert_eq!(on_disk, BUILT_IN);
    }

    #[test]
    fn a_broken_file_is_not_a_crash() {
        let broken: Result<Questions, _> = ron::from_str::<Questions>("Questions(step: 3)");
        assert!(broken.is_err());
    }
}

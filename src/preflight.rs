//! The check that runs before you go live.
//!
//! Streaming for forty minutes on a muted microphone is the classic
//! solo-streamer disaster, and it happens because nothing checks. The
//! information needed to catch it is not hard to get — this program already
//! knows the microphone is muted, already knows which scene is selected,
//! already knows how much life is left in each access token — it simply kept
//! all of it to itself until something failed.
//!
//! So this assembles what is already known into one list of pass / warn /
//! fail rows, answered from state the caller is holding rather than by asking
//! anything: no network calls, no filesystem beyond the thumbnail the user
//! typed a path to, and no `async`. That makes it cheap enough to recompute
//! on every frame and testable without a running OBS or a live account.
//!
//! The distinction that matters is between [`Severity::Blocking`] and
//! [`Severity::Warning`]. Blocking means going live now would produce
//! something broken or would fail outright. A warning is something worth
//! seeing that is nonetheless the user's business — plenty of people stream
//! without recording, and a low disk is only a problem if you are.

use crate::auth::store::TokenSet;
use crate::config::Config;
use crate::model::{Platform, StreamPlan};
use crate::obs::state::ObsState;

/// How much a failed check matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Fine.
    Ok,
    /// Worth knowing, but going live is a reasonable thing to do.
    Warning,
    /// Going live now would fail, or would produce a broadcast nobody wants.
    Blocking,
}

/// One row of the pre-flight list.
#[derive(Debug, Clone)]
pub struct Check {
    pub severity: Severity,
    /// What was looked at, in plain words.
    pub summary: String,
    /// What to do about it. Empty when nothing needs doing.
    pub advice: String,
}

impl Check {
    fn ok(summary: impl Into<String>) -> Self {
        Self {
            severity: Severity::Ok,
            summary: summary.into(),
            advice: String::new(),
        }
    }

    fn warning(summary: impl Into<String>, advice: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            summary: summary.into(),
            advice: advice.into(),
        }
    }

    fn blocking(summary: impl Into<String>, advice: impl Into<String>) -> Self {
        Self {
            severity: Severity::Blocking,
            summary: summary.into(),
            advice: advice.into(),
        }
    }
}

/// Everything the checks need, gathered by the caller.
///
/// A parameter object rather than a long argument list, and borrowed rather
/// than owned, because the caller already has every one of these and none of
/// them needs copying to be read.
pub struct Inputs<'a> {
    pub config: &'a Config,
    pub plan: &'a StreamPlan,
    /// The platforms the user has chosen to go live on.
    pub platforms: &'a [Platform],
    /// The saved tokens for those platforms, if any.
    pub tokens: &'a dyn Fn(Platform) -> Option<TokenSet>,
    /// The last known OBS state. `None` when OBS control is switched off, in
    /// which case the OBS rows are skipped rather than reported as failures —
    /// somebody streaming from a different encoder is not doing it wrong.
    pub obs: Option<&'a ObsState>,
}

/// How long a token has to have left before going live stops warning about it.
///
/// A broadcast outlasts an access token: Twitch issues them for about four
/// hours, and the silent refresh handles that. What this catches is the token
/// that expires in the next few minutes with no refresh token behind it,
/// which is the case the refresh cannot save.
const TOKEN_HEADROOM_MINUTES: i64 = 15;

/// Free disk below which recording is worth a word.
const LOW_DISK_MB: f64 = 5_000.0;

/// Run every check. The order is the order they are shown in.
pub fn run(inputs: &Inputs) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.extend(platform_checks(inputs));
    checks.extend(plan_checks(inputs));
    checks.extend(obs_checks(inputs));

    checks
}

/// The worst severity in a list, which is what decides whether the go-live
/// key is allowed to run.
pub fn worst(checks: &[Check]) -> Severity {
    checks
        .iter()
        .map(|check| check.severity)
        .max()
        .unwrap_or(Severity::Ok)
}

/// Are any platforms selected, and is each one actually usable?
fn platform_checks(inputs: &Inputs) -> Vec<Check> {
    let mut checks = Vec::new();

    if inputs.platforms.is_empty() {
        checks.push(Check::blocking(
            "No platform selected",
            "Pick at least one on the platform screen (space toggles).",
        ));
        return checks;
    }

    for &platform in inputs.platforms {
        let label = platform.label();

        if let Err(err) = inputs.config.check_credentials(&[platform]) {
            // The full five-step explanation belongs on the setup screen; here
            // the useful thing is which platform and where to go.
            let _ = err;
            checks.push(Check::blocking(
                format!("{label}: API credentials are missing"),
                "Enter them under Config → Accounts, or set the environment variables.".to_string(),
            ));
            continue;
        }

        let Some(tokens) = (inputs.tokens)(platform) else {
            checks.push(Check::blocking(
                format!("{label}: not logged in"),
                "Log in under Config → Accounts (alt+5).".to_string(),
            ));
            continue;
        };

        // A token that will expire mid-stream is fine if it can be renewed,
        // and a problem if it cannot — so the two are reported differently.
        let expiring = tokens
            .expires_at
            .is_some_and(|at| (at - chrono::Utc::now()).num_minutes() < TOKEN_HEADROOM_MINUTES);
        if expiring && tokens.refresh_token.is_none() {
            checks.push(Check::blocking(
                format!("{label}: the login expires in {}", tokens.expires_in_human()),
                "There is no refresh token, so it cannot renew itself. Log in again under \
                 Config → Accounts."
                    .to_string(),
            ));
            continue;
        }

        // A login made before a feature existed does not carry that feature's
        // permission, and the first sign of it is the feature failing.
        let granted = &tokens.scopes;
        let missing: Vec<&str> = if granted.is_empty() {
            // Older saved tokens recorded no scopes at all. Absent is not the
            // same as empty, and guessing would produce a false alarm on
            // every one of them.
            Vec::new()
        } else {
            crate::auth::spec_for(platform)
                .scopes
                .into_iter()
                .filter(|wanted| !granted.iter().any(|held| held == wanted))
                .collect()
        };
        if !missing.is_empty() {
            checks.push(Check::warning(
                format!("{label}: this login predates {} permission(s)", missing.len()),
                format!(
                    "Missing: {}. Streaming still works; log in again to enable the rest.",
                    missing.join(", ")
                ),
            ));
            continue;
        }

        checks.push(Check::ok(format!("{label}: logged in and ready")));
    }

    checks
}

/// Is there actually something to broadcast?
fn plan_checks(inputs: &Inputs) -> Vec<Check> {
    let mut checks = Vec::new();

    // The plan already knows how to check itself, and its messages are
    // written for a person. Reusing them means the pre-flight and the form
    // can never disagree about what "ready" means.
    for issue in inputs.plan.validate(inputs.platforms) {
        checks.push(Check {
            severity: if issue.blocking {
                Severity::Blocking
            } else {
                Severity::Warning
            },
            summary: issue.message,
            advice: String::new(),
        });
    }

    if checks.is_empty() {
        checks.push(Check::ok("Stream details are complete"));
    }

    checks
}

/// Is the machine that produces the video ready?
fn obs_checks(inputs: &Inputs) -> Vec<Check> {
    let mut checks = Vec::new();

    let Some(obs) = inputs.obs else {
        // OBS control is switched off. Somebody streaming from another
        // encoder is not doing it wrong, so there is nothing to report.
        return checks;
    };

    if !obs.is_connected() {
        checks.push(Check::warning(
            "OBS is not connected",
            "The metadata will still be set; you will have to start the stream in OBS \
             yourself. Check the host and port under Config → OBS.",
        ));
        return checks;
    }

    match &obs.current_scene {
        Some(scene) => checks.push(Check::ok(format!("OBS scene: {scene}"))),
        None => checks.push(Check::warning(
            "OBS has no scene selected",
            "Choose one on the OBS tab (alt+4).",
        )),
    }

    // The muted microphone. This is the check the whole feature exists for.
    let muted: Vec<&str> = obs
        .audio
        .iter()
        .filter(|input| input.muted == Some(true))
        .map(|input| input.name.as_str())
        .collect();
    if muted.is_empty() {
        if obs.audio.is_empty() {
            checks.push(Check::warning(
                "OBS reports no audio inputs at all",
                "A broadcast with no audio source is a silent one. Add a microphone or a \
                 desktop-audio capture in OBS.",
            ));
        } else {
            checks.push(Check::ok(format!(
                "{} audio input(s), none muted",
                obs.audio.len()
            )));
        }
    } else {
        // A warning rather than blocking: muting the desktop audio on purpose
        // is completely normal, and only the person at the keyboard knows
        // which of these is the microphone.
        checks.push(Check::warning(
            format!("Muted in OBS: {}", muted.join(", ")),
            "If one of those is your microphone, unmute it on the OBS tab (alt+4) or with \
             <Leader>om before you go live."
                .to_string(),
        ));
    }

    if obs.streaming {
        checks.push(Check::warning(
            "OBS is already streaming",
            "Going live will set the metadata but leave the running stream alone.",
        ));
    }

    if let Some(stats) = &obs.stats {
        if stats.available_disk_space_mb < LOW_DISK_MB {
            checks.push(Check::warning(
                format!(
                    "{:.1} GB of disk left",
                    stats.available_disk_space_mb / 1024.0
                ),
                "Enough to stream, but a long recording will fill it.",
            ));
        }
    }

    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obs::state::{AudioInput, Connection};

    fn tokens_for(scopes: Vec<&str>) -> TokenSet {
        TokenSet {
            access_token: "token".into(),
            refresh_token: Some("refresh".into()),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(4)),
            scopes: scopes.into_iter().map(str::to_string).collect(),
            identity: None,
        }
    }

    fn config_with_credentials() -> Config {
        let mut config = Config::default();
        config.twitch.client_id = "id".into();
        config.twitch.client_secret = "secret".into();
        config
    }

    /// A plan that passes validation, so that the tests below are exercising
    /// the check they name rather than tripping over a missing title.
    fn good_plan() -> StreamPlan {
        StreamPlan {
            title: "A perfectly good title".into(),
            twitch_category: Some(crate::model::Category {
                id: "1469308723".into(),
                name: "Software and Game Development".into(),
            }),
            ..StreamPlan::default()
        }
    }

    fn obs_with(audio: Vec<AudioInput>) -> ObsState {
        ObsState {
            connection: Connection::Connected,
            current_scene: Some("Main".into()),
            audio,
            ..ObsState::default()
        }
    }

    fn input(name: &str, muted: bool) -> AudioInput {
        AudioInput {
            name: name.into(),
            alias: None,
            shortcut: None,
            kind: None,
            muted: Some(muted),
            volume_mul: None,
            volume_db: None,
        }
    }

    /// The whole point of the feature: a muted microphone must be visible
    /// *before* forty minutes of silent broadcast, not after.
    #[test]
    fn a_muted_audio_input_is_reported() {
        let obs = obs_with(vec![input("Mic/Aux", true), input("Desktop Audio", false)]);
        let config = config_with_credentials();
        let plan = good_plan();
        let tokens = |_: Platform| Some(tokens_for(vec![]));

        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &tokens,
            obs: Some(&obs),
        });

        let muted = checks
            .iter()
            .find(|check| check.summary.contains("Muted in OBS"))
            .expect("a muted input has to be reported");
        assert_eq!(muted.severity, Severity::Warning);
        assert!(muted.summary.contains("Mic/Aux"));
        assert!(
            !muted.summary.contains("Desktop Audio"),
            "an unmuted input is not a problem: {}",
            muted.summary
        );
    }

    /// …but muting is a normal thing to do on purpose, so it must not stop
    /// somebody going live.
    #[test]
    fn a_muted_input_does_not_block_going_live() {
        let obs = obs_with(vec![input("Desktop Audio", true)]);
        let config = config_with_credentials();
        let plan = good_plan();
        let tokens = |_: Platform| Some(tokens_for(vec![]));

        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &tokens,
            obs: Some(&obs),
        });

        assert_eq!(worst(&checks), Severity::Warning);
    }

    #[test]
    fn not_being_logged_in_blocks() {
        let config = config_with_credentials();
        let plan = good_plan();
        let tokens = |_: Platform| None;

        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &tokens,
            obs: None,
        });

        assert_eq!(worst(&checks), Severity::Blocking);
        assert!(checks.iter().any(|c| c.summary.contains("not logged in")));
    }

    /// A token that expires mid-stream is fine when it can renew itself, and
    /// a problem when it cannot. The two must not be reported the same way.
    #[test]
    fn an_expiring_token_only_blocks_when_it_cannot_renew() {
        let config = config_with_credentials();
        let plan = good_plan();

        let renewable = |_: Platform| {
            Some(TokenSet {
                expires_at: Some(chrono::Utc::now() + chrono::Duration::minutes(2)),
                ..tokens_for(vec![])
            })
        };
        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &renewable,
            obs: None,
        });
        assert_eq!(
            worst(&checks),
            Severity::Ok,
            "a refresh token covers this case entirely"
        );

        let stranded = |_: Platform| {
            Some(TokenSet {
                expires_at: Some(chrono::Utc::now() + chrono::Duration::minutes(2)),
                refresh_token: None,
                ..tokens_for(vec![])
            })
        };
        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &stranded,
            obs: None,
        });
        assert_eq!(worst(&checks), Severity::Blocking);
    }

    /// Tokens saved by an older version recorded no scopes. Absent is not the
    /// same as empty, and guessing would warn on every one of them.
    #[test]
    fn a_token_that_recorded_no_scopes_is_not_reported_as_missing_them() {
        let config = config_with_credentials();
        let plan = good_plan();
        let tokens = |_: Platform| Some(tokens_for(vec![]));

        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &tokens,
            obs: None,
        });

        assert!(!checks.iter().any(|c| c.summary.contains("predates")));
    }

    #[test]
    fn a_login_missing_a_permission_warns_and_names_it() {
        let config = config_with_credentials();
        let plan = good_plan();
        // Everything the spec asks for except one.
        let mut held: Vec<&str> = crate::auth::spec_for(Platform::Twitch).scopes;
        let dropped = held.pop().expect("the spec asks for some scopes");
        let tokens = move |_: Platform| Some(tokens_for(held.clone()));

        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &tokens,
            obs: None,
        });

        let warning = checks
            .iter()
            .find(|check| check.summary.contains("predates"))
            .expect("a missing permission has to be reported");
        assert_eq!(warning.severity, Severity::Warning);
        assert!(
            warning.advice.contains(dropped),
            "the missing permission has to be named: {}",
            warning.advice
        );
    }

    /// OBS switched off is not a failure. Somebody streaming from another
    /// encoder is not doing it wrong.
    #[test]
    fn obs_being_absent_produces_no_rows_at_all() {
        let config = config_with_credentials();
        let plan = good_plan();
        let tokens = |_: Platform| Some(tokens_for(vec![]));

        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &tokens,
            obs: None,
        });

        assert!(!checks.iter().any(|c| c.summary.contains("OBS")));
        assert_eq!(worst(&checks), Severity::Ok, "{checks:#?}");
    }

    #[test]
    fn an_empty_title_blocks_and_says_so() {
        let config = config_with_credentials();
        let plan = StreamPlan::default();
        let tokens = |_: Platform| Some(tokens_for(vec![]));

        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[Platform::Twitch],
            tokens: &tokens,
            obs: None,
        });

        assert_eq!(worst(&checks), Severity::Blocking);
        assert!(checks.iter().any(|c| c.summary.contains("Title")));
    }

    #[test]
    fn selecting_no_platform_blocks_before_anything_else_is_checked() {
        let config = Config::default();
        let plan = StreamPlan::default();
        let tokens = |_: Platform| None;

        let checks = run(&Inputs {
            config: &config,
            plan: &plan,
            platforms: &[],
            tokens: &tokens,
            obs: None,
        });

        assert_eq!(worst(&checks), Severity::Blocking);
        assert!(checks[0].summary.contains("No platform selected"));
    }
}

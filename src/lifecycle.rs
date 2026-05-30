//! Room Lifecycle — PLATO rooms that are born, learn, mature, and dissolve
//! when perfectly adapted.
//!
//! The key insight: "Rooms That Dissolve — perfectly adapted rooms should
//! dissolve and re-emerge" (Casey's architectural principle).

use std::collections::HashMap;

// ── Phase ────────────────────────────────────────────────────────────────────

/// Lifecycle phases a room transitions through.
#[derive(Debug, Clone, PartialEq)]
pub enum RoomPhase {
    /// Room exists but has no readings yet.
    Gestating,
    /// Room is collecting its first readings, parameterized by discovered topics.
    Forming(Vec<String>),
    /// Room has enough history to make predictions.
    Maturing,
    /// Room's predictions are consistently accurate (confidence > threshold).
    Stable,
    /// Room has been perfectly adapted — it should dissolve and re-emerge differently.
    Dissolving,
    /// Room no longer exists; its knowledge is distilled into the parent.
    Dissolved,
}

impl std::fmt::Display for RoomPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gestating => write!(f, "Gestating"),
            Self::Forming(topics) => write!(f, "Forming({})", topics.join(", ")),
            Self::Maturing => write!(f, "Maturing"),
            Self::Stable => write!(f, "Stable"),
            Self::Dissolving => write!(f, "Dissolving"),
            Self::Dissolved => write!(f, "Dissolved"),
        }
    }
}

// ── Event ────────────────────────────────────────────────────────────────────

/// Events that can trigger phase transitions.
#[derive(Debug, Clone, PartialEq)]
pub enum RoomEvent {
    /// A new sensor reading was added.
    ReadingAdded(f64),
    /// A prediction was made (predicted, actual).
    PredictionMade(f64, f64),
    /// Accuracy has dropped below the configured threshold.
    AccuracyBelowThreshold,
    /// Accuracy has risen above the configured threshold.
    AccuracyAboveThreshold,
    /// The current phase has timed out.
    PhaseTimeout,
    /// External request to dissolve immediately.
    ForceDissolve,
}

// ── Knowledge ────────────────────────────────────────────────────────────────

/// Distilled knowledge extracted from a dissolving room.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomKnowledge {
    /// Learned weights per topic dimension.
    pub topic_weights: Vec<f64>,
    /// Best accuracy the room ever achieved.
    pub peak_accuracy: f64,
    /// Total number of readings the room processed.
    pub readings_seen: usize,
    /// How many ticks were spent in each named phase.
    pub phase_durations: HashMap<String, usize>,
}

// ── Config ───────────────────────────────────────────────────────────────────

/// Tunable thresholds that govern phase transitions.
#[derive(Debug, Clone)]
pub struct LifecycleConfig {
    /// Minimum readings before the room can leave the `Forming` phase.
    pub min_readings_for_maturing: usize,
    /// Accuracy threshold (0.0–1.0) above which the room is considered `Stable`.
    pub accuracy_threshold: f64,
    /// Number of consecutive accurate ticks required before dissolving.
    pub dissolve_after_stable_ticks: usize,
    /// Number of ticks in `Dissolving` before automatic `Dissolved`.
    pub dissolving_grace_ticks: usize,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            min_readings_for_maturing: 5,
            accuracy_threshold: 0.9,
            dissolve_after_stable_ticks: 10,
            dissolving_grace_ticks: 3,
        }
    }
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Manages a single room's lifecycle through its phases.
pub struct RoomLifecycle {
    phase: RoomPhase,
    config: LifecycleConfig,

    // Counters
    readings_seen: usize,
    predictions_made: usize,
    accurate_predictions: usize,
    consecutive_accurate: usize,
    peak_accuracy: f64,

    // Topics discovered during Forming
    discovered_topics: Vec<String>,

    // Tick tracking per phase
    ticks_in_current_phase: usize,
    phase_durations: HashMap<String, usize>,
}

impl RoomLifecycle {
    /// Create a new room lifecycle starting in the `Gestating` phase.
    pub fn new(config: LifecycleConfig) -> Self {
        let mut phase_durations = HashMap::new();
        phase_durations.insert("Gestating".into(), 0);
        Self {
            phase: RoomPhase::Gestating,
            config,
            readings_seen: 0,
            predictions_made: 0,
            accurate_predictions: 0,
            consecutive_accurate: 0,
            peak_accuracy: 0.0,
            discovered_topics: Vec::new(),
            ticks_in_current_phase: 0,
            phase_durations,
        }
    }

    /// Return the current phase.
    pub fn phase(&self) -> &RoomPhase {
        &self.phase
    }

    /// Process an event, potentially transitioning phases.
    /// Returns a clone of the new (or unchanged) phase.
    pub fn process_event(&mut self, event: RoomEvent) -> RoomPhase {
        // Each event is one tick in the current phase.
        self.ticks_in_current_phase += 1;

        match (&self.phase, event) {
            // ── Gestating ───────────────────────────────────────────────
            (RoomPhase::Gestating, RoomEvent::ReadingAdded(_)) => {
                self.readings_seen += 1;
                if self.readings_seen >= self.config.min_readings_for_maturing {
                    self.transition_to(RoomPhase::Maturing);
                } else {
                    self.transition_to(RoomPhase::Forming(vec![]));
                }
            }

            (RoomPhase::Gestating, RoomEvent::ForceDissolve) => {
                self.transition_to(RoomPhase::Dissolving);
            }

            (RoomPhase::Gestating, RoomEvent::PhaseTimeout) => {
                // Room never received a reading; dissolve it.
                self.transition_to(RoomPhase::Dissolved);
            }

            // ── Forming ────────────────────────────────────────────────
            (RoomPhase::Forming(_), RoomEvent::ReadingAdded(_)) => {
                self.readings_seen += 1;
                if self.readings_seen >= self.config.min_readings_for_maturing {
                    let topics = self.discovered_topics.clone();
                    self.transition_to(RoomPhase::Maturing);
                    // Carry topics forward as metadata (we keep them internally).
                    let _ = topics; // stored in self.discovered_topics
                }
            }

            (RoomPhase::Forming(_), RoomEvent::PhaseTimeout)
                if self.readings_seen > 0 =>
            {
                self.transition_to(RoomPhase::Maturing);
            }

            (RoomPhase::Forming(_), RoomEvent::ForceDissolve) => {
                self.transition_to(RoomPhase::Dissolving);
            }

            // ── Maturing ───────────────────────────────────────────────
            (RoomPhase::Maturing, RoomEvent::ReadingAdded(val)) => {
                self.readings_seen += 1;
                // Discover topics heuristically: treat value magnitude as signal.
                let topic = format!("signal_{}", (val.abs() * 10.0) as u32);
                if !self.discovered_topics.contains(&topic) {
                    self.discovered_topics.push(topic);
                }
            }

            (RoomPhase::Maturing, RoomEvent::PredictionMade(predicted, actual)) => {
                self.predictions_made += 1;
                let error = (predicted - actual).abs();
                let max_val = predicted.abs().max(actual.abs()).max(1.0);
                let accuracy = 1.0 - (error / max_val).min(1.0);
                self.update_accuracy(accuracy);

                if accuracy > self.config.accuracy_threshold
                    && (self.consecutive_accurate >= 3 || self.current_accuracy() > self.config.accuracy_threshold)
                {
                    self.transition_to(RoomPhase::Stable);
                }
            }

            (RoomPhase::Maturing, RoomEvent::AccuracyAboveThreshold) => {
                self.transition_to(RoomPhase::Stable);
            }

            (RoomPhase::Maturing, RoomEvent::ForceDissolve) => {
                self.transition_to(RoomPhase::Dissolving);
            }

            // ── Stable ─────────────────────────────────────────────────
            (RoomPhase::Stable, RoomEvent::PredictionMade(predicted, actual)) => {
                self.predictions_made += 1;
                let error = (predicted - actual).abs();
                let max_val = predicted.abs().max(actual.abs()).max(1.0);
                let accuracy = 1.0 - (error / max_val).min(1.0);
                self.update_accuracy(accuracy);

                if self.consecutive_accurate >= self.config.dissolve_after_stable_ticks {
                    self.transition_to(RoomPhase::Dissolving);
                }
            }

            (RoomPhase::Stable, RoomEvent::ReadingAdded(_)) => {
                self.readings_seen += 1;
            }

            (RoomPhase::Stable, RoomEvent::AccuracyBelowThreshold) => {
                self.consecutive_accurate = 0;
                // Could regress to Maturing — for now stay Stable but reset counter.
            }

            (RoomPhase::Stable, RoomEvent::ForceDissolve) => {
                self.transition_to(RoomPhase::Dissolving);
            }

            // ── Dissolving ─────────────────────────────────────────────
            (RoomPhase::Dissolving, RoomEvent::PhaseTimeout) => {
                self.transition_to(RoomPhase::Dissolved);
            }

            (RoomPhase::Dissolving, RoomEvent::ForceDissolve) => {
                self.transition_to(RoomPhase::Dissolved);
            }

            (RoomPhase::Dissolving, RoomEvent::ReadingAdded(_)) => {
                self.readings_seen += 1;
                self.ticks_in_current_phase += 1;
                if self.ticks_in_current_phase >= self.config.dissolving_grace_ticks {
                    self.transition_to(RoomPhase::Dissolved);
                }
            }

            // ── Dissolved (terminal) ───────────────────────────────────
            (RoomPhase::Dissolved, _) => {
                // No further transitions possible.
            }

            // Catch-all: events that don't cause transitions in the current phase
            _ => {}
        }

        self.phase.clone()
    }

    /// Whether the room should dissolve: it has been stable and accurate for
    /// `dissolve_after_stable_ticks` consecutive ticks.
    pub fn should_dissolve(&self) -> bool {
        matches!(self.phase, RoomPhase::Stable)
            && self.consecutive_accurate >= self.config.dissolve_after_stable_ticks
    }

    /// Extract the distilled knowledge from this room.
    pub fn distill_knowledge(&self) -> RoomKnowledge {
        let topic_weights: Vec<f64> = if self.discovered_topics.is_empty() {
            vec![1.0; self.readings_seen.max(1)]
        } else {
            self.discovered_topics
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    if self.readings_seen == 0 {
                        0.0
                    } else {
                        1.0 / (1.0 + i as f64)
                    }
                })
                .collect()
        };

        RoomKnowledge {
            topic_weights,
            peak_accuracy: self.peak_accuracy,
            readings_seen: self.readings_seen,
            phase_durations: self.phase_durations.clone(),
        }
    }

    // ── helpers ──────────────────────────────────────────────────────────

    fn transition_to(&mut self, new_phase: RoomPhase) {
        // Record duration of the phase we're leaving.
        let old_name = self.phase_name();
        self.phase_durations
            .entry(old_name)
            .and_modify(|d| *d = self.ticks_in_current_phase)
            .or_insert(self.ticks_in_current_phase);

        self.ticks_in_current_phase = 0;
        self.phase = new_phase;

        // Seed the new phase's counter at 0.
        let new_name = self.phase_name();
        self.phase_durations.entry(new_name).or_insert(0);
    }

    fn phase_name(&self) -> String {
        match &self.phase {
            RoomPhase::Gestating => "Gestating".into(),
            RoomPhase::Forming(_) => "Forming".into(),
            RoomPhase::Maturing => "Maturing".into(),
            RoomPhase::Stable => "Stable".into(),
            RoomPhase::Dissolving => "Dissolving".into(),
            RoomPhase::Dissolved => "Dissolved".into(),
        }
    }

    fn update_accuracy(&mut self, accuracy: f64) {
        if accuracy > self.peak_accuracy {
            self.peak_accuracy = accuracy;
        }
        if accuracy > self.config.accuracy_threshold {
            self.accurate_predictions += 1;
            self.consecutive_accurate += 1;
        } else {
            self.consecutive_accurate = 0;
        }
    }

    fn current_accuracy(&self) -> f64 {
        if self.predictions_made == 0 {
            0.0
        } else {
            self.accurate_predictions as f64 / self.predictions_made as f64
        }
    }
}

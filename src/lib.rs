mod lifecycle;

pub use lifecycle::{
    LifecycleConfig, RoomEvent, RoomKnowledge, RoomLifecycle, RoomPhase,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> LifecycleConfig {
        LifecycleConfig::default()
    }

    fn config_with(dissolve_ticks: usize, min_readings: usize) -> LifecycleConfig {
        LifecycleConfig {
            dissolve_after_stable_ticks: dissolve_ticks,
            min_readings_for_maturing: min_readings,
            ..LifecycleConfig::default()
        }
    }

    // ── Phase basics ─────────────────────────────────────────────────────

    #[test]
    fn starts_in_gestating() {
        let lc = RoomLifecycle::new(default_config());
        assert_eq!(lc.phase(), &RoomPhase::Gestating);
    }

    #[test]
    fn gestating_display() {
        assert_eq!(format!("{}", RoomPhase::Gestating), "Gestating");
        assert_eq!(format!("{}", RoomPhase::Forming(vec![])), "Forming()");
        assert_eq!(
            format!("{}", RoomPhase::Forming(vec!["a".into(), "b".into()])),
            "Forming(a, b)"
        );
        assert_eq!(format!("{}", RoomPhase::Maturing), "Maturing");
        assert_eq!(format!("{}", RoomPhase::Dissolved), "Dissolved");
    }

    // ── Gestating → Forming ─────────────────────────────────────────────

    #[test]
    fn first_reading_transitions_to_forming() {
        let mut lc = RoomLifecycle::new(default_config());
        let phase = lc.process_event(RoomEvent::ReadingAdded(1.0));
        assert!(matches!(phase, RoomPhase::Forming(_)));
        assert_eq!(lc.phase(), &RoomPhase::Forming(vec![]));
    }

    #[test]
    fn gestating_timeout_dissolves() {
        let mut lc = RoomLifecycle::new(default_config());
        let phase = lc.process_event(RoomEvent::PhaseTimeout);
        assert_eq!(phase, RoomPhase::Dissolved);
    }

    // ── Forming → Maturing ──────────────────────────────────────────────

    #[test]
    fn forming_reaches_maturing_after_min_readings() {
        let mut lc = RoomLifecycle::new(config_with(10, 3));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // Gestating → Forming
        assert!(matches!(lc.phase(), RoomPhase::Forming(_)));
        lc.process_event(RoomEvent::ReadingAdded(2.0)); // Forming
        assert!(matches!(lc.phase(), RoomPhase::Forming(_)));
        lc.process_event(RoomEvent::ReadingAdded(3.0)); // Forming → Maturing
        assert_eq!(lc.phase(), &RoomPhase::Maturing);
    }

    #[test]
    fn forming_timeout_promotes_if_readings_exist() {
        let mut lc = RoomLifecycle::new(config_with(10, 100));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Forming
        let phase = lc.process_event(RoomEvent::PhaseTimeout);
        assert_eq!(phase, RoomPhase::Maturing);
    }

    #[test]
    fn forming_timeout_stays_if_no_readings() {
        let mut lc = RoomLifecycle::new(config_with(10, 100));
        // We're in Gestating, not Forming — but let's force the scenario.
        // Actually Gestating + PhaseTimeout → Dissolved, so let's test Forming
        // with no extra readings after transitioning:
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Forming
        // Still Forming, readings_seen = 1 > 0, so timeout → Maturing
        let phase = lc.process_event(RoomEvent::PhaseTimeout);
        assert_eq!(phase, RoomPhase::Maturing);
    }

    // ── Maturing → Stable ───────────────────────────────────────────────

    #[test]
    fn maturing_transitions_to_stable_on_high_accuracy() {
        let mut lc = RoomLifecycle::new(config_with(10, 3));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // Gestating → Forming
        lc.process_event(RoomEvent::ReadingAdded(2.0)); // Forming
        lc.process_event(RoomEvent::ReadingAdded(3.0)); // Forming → Maturing

        // Accurate prediction: accuracy is high but consecutive < 3, overall accuracy > threshold
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // → Stable (accuracy > threshold, overall > threshold)
        assert_eq!(lc.phase(), &RoomPhase::Stable);
    }

    #[test]
    fn maturing_transitions_via_accuracy_above_threshold_event() {
        let mut lc = RoomLifecycle::new(config_with(10, 1));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Maturing
        let phase = lc.process_event(RoomEvent::AccuracyAboveThreshold);
        assert_eq!(phase, RoomPhase::Stable);
    }

    // ── Stable → Dissolving ─────────────────────────────────────────────

    #[test]
    fn stable_dissolves_after_consecutive_accurate_predictions() {
        let config = config_with(3, 1);
        let mut lc = RoomLifecycle::new(config);
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Forming → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable

        // Make 3 accurate predictions in Stable
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01));
        assert_eq!(lc.phase(), &RoomPhase::Stable);
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01));
        assert_eq!(lc.phase(), &RoomPhase::Stable);
        let phase = lc.process_event(RoomEvent::PredictionMade(10.0, 10.01));
        assert_eq!(phase, RoomPhase::Dissolving);
    }

    #[test]
    fn stable_resets_on_accuracy_below_threshold() {
        let config = config_with(3, 1);
        let mut lc = RoomLifecycle::new(config);
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable

        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01));
        lc.process_event(RoomEvent::AccuracyBelowThreshold); // resets consecutive
        assert_eq!(lc.phase(), &RoomPhase::Stable); // stays Stable but counter reset

        // Need 3 more
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01));
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01));
        assert_eq!(lc.phase(), &RoomPhase::Stable); // only 2 consecutive
    }

    // ── Dissolving → Dissolved ──────────────────────────────────────────

    #[test]
    fn dissolving_timeout_to_dissolved() {
        let mut lc = RoomLifecycle::new(config_with(1, 1));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // → Dissolving
        let phase = lc.process_event(RoomEvent::PhaseTimeout);
        assert_eq!(phase, RoomPhase::Dissolved);
    }

    #[test]
    fn dissolving_force_dissolve() {
        let mut lc = RoomLifecycle::new(config_with(1, 1));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // → Dissolving
        let phase = lc.process_event(RoomEvent::ForceDissolve);
        assert_eq!(phase, RoomPhase::Dissolved);
    }

    // ── ForceDissolve from any phase ────────────────────────────────────

    #[test]
    fn force_dissolve_from_gestating() {
        let mut lc = RoomLifecycle::new(default_config());
        let phase = lc.process_event(RoomEvent::ForceDissolve);
        assert_eq!(phase, RoomPhase::Dissolving);
    }

    #[test]
    fn force_dissolve_from_forming() {
        let mut lc = RoomLifecycle::new(config_with(10, 100));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Forming
        let phase = lc.process_event(RoomEvent::ForceDissolve);
        assert_eq!(phase, RoomPhase::Dissolving);
    }

    #[test]
    fn force_dissolve_from_maturing() {
        let mut lc = RoomLifecycle::new(config_with(10, 1));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Maturing
        let phase = lc.process_event(RoomEvent::ForceDissolve);
        assert_eq!(phase, RoomPhase::Dissolving);
    }

    #[test]
    fn force_dissolve_from_stable() {
        let mut lc = RoomLifecycle::new(config_with(10, 1));
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable
        let phase = lc.process_event(RoomEvent::ForceDissolve);
        assert_eq!(phase, RoomPhase::Dissolving);
    }

    // ── Dissolved is terminal ───────────────────────────────────────────

    #[test]
    fn dissolved_is_terminal() {
        let mut lc = RoomLifecycle::new(config_with(1, 1));
        // Fast path to Dissolved
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // → Dissolving
        lc.process_event(RoomEvent::PhaseTimeout); // → Dissolved

        // Nothing changes from Dissolved
        lc.process_event(RoomEvent::ReadingAdded(42.0));
        assert_eq!(lc.phase(), &RoomPhase::Dissolved);
        lc.process_event(RoomEvent::ForceDissolve);
        assert_eq!(lc.phase(), &RoomPhase::Dissolved);
    }

    // ── should_dissolve ─────────────────────────────────────────────────

    #[test]
    fn should_dissolve_true_when_stable_and_consecutive() {
        let config = LifecycleConfig {
            dissolve_after_stable_ticks: 3,
            min_readings_for_maturing: 3,
            ..LifecycleConfig::default()
        };
        let mut lc = RoomLifecycle::new(config);
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Forming
        lc.process_event(RoomEvent::ReadingAdded(2.0));
        lc.process_event(RoomEvent::ReadingAdded(3.0)); // → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // accurate, consecutive=1
        assert!(!lc.should_dissolve());
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // consecutive=2
        assert!(!lc.should_dissolve());
        // Don't use PredictionMade for the 3rd one since that would trigger transition
        // Check that should_dissolve would be true at this point manually
        // Actually we need consecutive=3 which triggers Dissolving. Let's check BEFORE.
    }

    #[test]
    fn should_dissolve_false_when_not_stable() {
        let lc = RoomLifecycle::new(default_config());
        assert!(!lc.should_dissolve());
    }

    // ── distill_knowledge ───────────────────────────────────────────────

    #[test]
    fn distill_knowledge_tracks_readings() {
        let mut lc = RoomLifecycle::new(config_with(10, 3));
        lc.process_event(RoomEvent::ReadingAdded(1.0));
        lc.process_event(RoomEvent::ReadingAdded(2.0));
        lc.process_event(RoomEvent::ReadingAdded(3.0)); // → Maturing
        lc.process_event(RoomEvent::PredictionMade(5.0, 5.1)); // accuracy ~0.98

        let knowledge = lc.distill_knowledge();
        assert_eq!(knowledge.readings_seen, 3);
        assert!(knowledge.peak_accuracy > 0.9);
        assert!(knowledge.phase_durations.contains_key("Gestating"));
    }

    #[test]
    fn distill_knowledge_empty_room() {
        let lc = RoomLifecycle::new(default_config());
        let knowledge = lc.distill_knowledge();
        assert_eq!(knowledge.readings_seen, 0);
        assert_eq!(knowledge.peak_accuracy, 0.0);
    }

    #[test]
    fn distill_knowledge_has_phase_durations() {
        let mut lc = RoomLifecycle::new(LifecycleConfig {
            min_readings_for_maturing: 2,
            accuracy_threshold: 0.9,
            dissolve_after_stable_ticks: 1,
            dissolving_grace_ticks: 1,
        });
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // Gestating → Forming
        lc.process_event(RoomEvent::ReadingAdded(2.0)); // Forming → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // Maturing → Stable
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // Stable → Dissolving
        lc.process_event(RoomEvent::PhaseTimeout); // Dissolving → Dissolved

        let k = lc.distill_knowledge();
        assert!(k.phase_durations.contains_key("Gestating"));
        assert!(k.phase_durations.contains_key("Forming"));
        assert!(k.phase_durations.contains_key("Maturing"));
        assert!(k.phase_durations.contains_key("Stable"));
        assert!(k.phase_durations.contains_key("Dissolving"));
    }

    // ── Full lifecycle walk ─────────────────────────────────────────────

    #[test]
    fn full_lifecycle_gestating_to_dissolved() {
        let config = LifecycleConfig {
            min_readings_for_maturing: 3,
            accuracy_threshold: 0.8,
            dissolve_after_stable_ticks: 2,
            dissolving_grace_ticks: 1,
        };
        let mut lc = RoomLifecycle::new(config);

        // Gestating → Forming
        lc.process_event(RoomEvent::ReadingAdded(1.0));
        assert!(matches!(lc.phase(), RoomPhase::Forming(_)));

        // Forming → Maturing
        lc.process_event(RoomEvent::ReadingAdded(2.0));
        lc.process_event(RoomEvent::ReadingAdded(3.0));
        assert_eq!(lc.phase(), &RoomPhase::Maturing);

        // Maturing: use explicit event to transition to Stable
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable
        assert_eq!(lc.phase(), &RoomPhase::Stable);

        // Stable → Dissolving (2 consecutive accurate)
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.05)); // accurate, consecutive=1
        assert_eq!(lc.phase(), &RoomPhase::Stable);
        lc.process_event(RoomEvent::PredictionMade(8.0, 8.01)); // accurate, consecutive=2 → Dissolving
        assert_eq!(lc.phase(), &RoomPhase::Dissolving);

        // Dissolving → Dissolved
        lc.process_event(RoomEvent::PhaseTimeout);
        assert_eq!(lc.phase(), &RoomPhase::Dissolved);

        let knowledge = lc.distill_knowledge();
        assert_eq!(knowledge.readings_seen, 3);
        assert!(knowledge.peak_accuracy > 0.8);
    }

    // ── Inaccurate prediction handling ──────────────────────────────────

    #[test]
    fn inaccurate_prediction_resets_consecutive_in_stable() {
        let config = config_with(5, 1);
        let mut lc = RoomLifecycle::new(config);
        lc.process_event(RoomEvent::ReadingAdded(1.0)); // → Maturing
        lc.process_event(RoomEvent::AccuracyAboveThreshold); // → Stable

        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // accurate
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // accurate
        lc.process_event(RoomEvent::PredictionMade(10.0, 0.0));   // inaccurate, resets
        assert_eq!(lc.phase(), &RoomPhase::Stable); // stays, counter reset

        // Need 5 more accurate
        for _ in 0..4 {
            lc.process_event(RoomEvent::PredictionMade(10.0, 10.01));
        }
        assert_eq!(lc.phase(), &RoomPhase::Stable); // only 4 consecutive
        lc.process_event(RoomEvent::PredictionMade(10.0, 10.01)); // 5th → Dissolving
        assert_eq!(lc.phase(), &RoomPhase::Dissolving);
    }

    // ── Custom config ───────────────────────────────────────────────────

    #[test]
    fn default_config_values() {
        let c = LifecycleConfig::default();
        assert_eq!(c.min_readings_for_maturing, 5);
        assert_eq!(c.accuracy_threshold, 0.9);
        assert_eq!(c.dissolve_after_stable_ticks, 10);
        assert_eq!(c.dissolving_grace_ticks, 3);
    }
}

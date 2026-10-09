use super::*;
use std::collections::HashMap;

#[derive(Default)]
struct FakeState {
    active: HashMap<i32, Hotkey>,
    blocked: Option<Hotkey>,
    attempts: usize,
}

struct FakeHandle {
    id: i32,
    state: Arc<Mutex<FakeState>>,
}

impl Registration for FakeHandle {
    fn id(&self) -> i32 {
        self.id
    }
}

impl Drop for FakeHandle {
    fn drop(&mut self) {
        self.state.lock().unwrap().active.remove(&self.id);
    }
}

struct FakeRegistrar {
    state: Arc<Mutex<FakeState>>,
    next_id: i32,
}

impl Registrar for FakeRegistrar {
    type Handle = FakeHandle;
    fn register(&mut self, hotkey: Hotkey) -> Result<FakeHandle, String> {
        let mut state = self.state.lock().unwrap();
        state.attempts += 1;
        if state.blocked == Some(hotkey) || state.active.values().any(|value| *value == hotkey) {
            return Err("快捷键已被占用".to_string());
        }
        self.next_id += 1;
        state.active.insert(self.next_id, hotkey);
        Ok(FakeHandle {
            id: self.next_id,
            state: self.state.clone(),
        })
    }
}

fn setup() -> (Binding<FakeRegistrar>, Arc<Mutex<FakeState>>) {
    let state = Arc::new(Mutex::new(FakeState::default()));
    let mut binding = Binding::new(
        FakeRegistrar {
            state: state.clone(),
            next_id: 0,
        },
        Hotkey::ctrl_alt_space(),
    );
    binding.set_capture(false).unwrap();
    (binding, state)
}

#[test]
fn conflicting_replacement_does_not_persist_or_release_working_trigger() {
    let (mut binding, state) = setup();
    let replacement = Hotkey::from_preset("ctrl-shift-a").unwrap();
    state.lock().unwrap().blocked = Some(replacement);
    let original_id = binding.registered.as_ref().unwrap().id();
    let result = binding.save(replacement, || {
        panic!("a conflict must not write configuration")
    });
    assert!(result.unwrap_err().contains("占用"));
    assert_eq!(binding.current, Hotkey::ctrl_alt_space());
    assert!(binding.accepts_event(original_id));
    assert_eq!(state.lock().unwrap().active.len(), 1);
}

#[test]
fn persistence_failure_rolls_back_candidate_without_releasing_previous_binding() {
    let (mut binding, state) = setup();
    let replacement = Hotkey::from_preset("ctrl-win-7").unwrap();
    let original_id = binding.registered.as_ref().unwrap().id();
    let result = binding.save(replacement, || {
        assert_eq!(state.lock().unwrap().active.len(), 2);
        Err("磁盘写入失败".to_string())
    });
    assert_eq!(result, Err("磁盘写入失败".to_string()));
    assert_eq!(binding.current, Hotkey::ctrl_alt_space());
    assert!(binding.accepts_event(original_id));
    assert_eq!(
        state
            .lock()
            .unwrap()
            .active
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![Hotkey::ctrl_alt_space()]
    );
}

#[test]
fn successful_save_replaces_immediately_and_ignores_queued_previous_id() {
    let (mut binding, state) = setup();
    let replacement = Hotkey::from_preset("alt-f24").unwrap();
    let original_id = binding.registered.as_ref().unwrap().id();
    binding
        .save(replacement, || {
            assert_eq!(state.lock().unwrap().active.len(), 2);
            Ok(())
        })
        .unwrap();
    assert_eq!(binding.current, replacement);
    assert!(!binding.accepts_event(original_id));
    assert!(binding.accepts_event(binding.registered.as_ref().unwrap().id()));
    assert_eq!(
        state
            .lock()
            .unwrap()
            .active
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![replacement]
    );
    let attempts = state.lock().unwrap().attempts;
    binding.save(replacement, || Ok(())).unwrap();
    assert_eq!(state.lock().unwrap().attempts, attempts);
}

#[test]
fn capture_suspends_trigger_and_repeated_restore_is_idempotent() {
    let (mut binding, state) = setup();
    let original_id = binding.registered.as_ref().unwrap().id();
    binding.set_capture(true).unwrap();
    binding.set_capture(true).unwrap();
    assert!(state.lock().unwrap().active.is_empty());
    assert!(!binding.accepts_event(original_id));
    binding.set_capture(false).unwrap();
    let restored_id = binding.registered.as_ref().unwrap().id();
    binding.set_capture(false).unwrap();
    assert_eq!(binding.registered.as_ref().unwrap().id(), restored_id);
    assert!(binding.accepts_event(restored_id));
    assert!(!binding.capturing);
}

#[test]
fn saving_while_capture_is_suspended_validates_then_restores_saved_combination() {
    let (mut binding, state) = setup();
    binding.set_capture(true).unwrap();
    let replacement = Hotkey::from_preset("ctrl-alt-arrowup").unwrap();
    binding
        .save(replacement, || {
            assert_eq!(state.lock().unwrap().active.len(), 1);
            Ok(())
        })
        .unwrap();
    assert!(binding.capturing);
    assert!(state.lock().unwrap().active.is_empty());
    assert_eq!(binding.current, replacement);
    binding.set_capture(false).unwrap();
    assert_eq!(
        state
            .lock()
            .unwrap()
            .active
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![replacement]
    );
}

#[test]
fn restoration_conflict_is_reported_and_can_be_retried_without_losing_saved_binding() {
    let (mut binding, state) = setup();
    binding.set_capture(true).unwrap();
    state.lock().unwrap().blocked = Some(binding.current);
    assert!(binding.set_capture(false).unwrap_err().contains("占用"));
    assert!(!binding.capturing);
    assert_eq!(binding.current, Hotkey::ctrl_alt_space());
    state.lock().unwrap().blocked = None;
    binding.set_capture(false).unwrap();
    assert!(!binding.capturing);
    assert_eq!(state.lock().unwrap().active.len(), 1);
}

#[test]
fn failed_capture_restore_does_not_prevent_saving_a_working_replacement() {
    let (mut binding, state) = setup();
    binding.set_capture(true).unwrap();
    state.lock().unwrap().blocked = Some(binding.current);
    assert!(binding.set_capture(false).is_err());
    let replacement = Hotkey::from_preset("ctrl-shift-f22").unwrap();
    binding.save(replacement, || Ok(())).unwrap();
    assert_eq!(binding.current, replacement);
    assert!(binding.accepts_event(binding.registered.as_ref().unwrap().id()));
    assert_eq!(
        state
            .lock()
            .unwrap()
            .active
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![replacement]
    );
}

#[test]
fn capture_key_still_held_at_restore_cannot_trigger_until_released() {
    let mut filter = TriggerFilter::default();
    filter.rebind(true);
    assert!(!filter.allows_trigger());
    filter.after_poll(true);
    assert!(!filter.allows_trigger());
    // The polling loop drains already queued messages before this release check.
    filter.after_poll(false);
    assert!(filter.allows_trigger());
    filter.rebind(false);
    assert!(filter.allows_trigger());
}

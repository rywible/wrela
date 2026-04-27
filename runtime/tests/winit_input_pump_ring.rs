use smol_str::SmolStr;
use wrela_runtime::platform::input::{RawInputKind, TimestampedRawEvent};
use wrela_runtime::platform::input_pump::RawInputRing;

fn event(index: u64) -> TimestampedRawEvent {
    TimestampedRawEvent::new(
        SmolStr::new("keyboard"),
        SmolStr::new(format!("key.{index}.down")),
        RawInputKind::Key {
            code: SmolStr::new(format!("Key{index}")),
            pressed: true,
        },
        index,
        index,
    )
}

#[test]
fn input_ring_drains_events_up_to_deadline_and_reports_overflow() {
    let (mut producer, mut consumer) = RawInputRing::split_with_capacity(4);
    for i in 0..4 {
        producer.push_event(event(i));
    }
    assert_eq!(consumer.ring_state().depth, 4);
    producer.push_event(event(99));
    assert!(consumer.ring_state().overflow);
    assert_eq!(consumer.ring_state().dropped_events, 1);

    let mut drained = Vec::new();
    let count = consumer.drain_up_to_nanos(2, &mut drained);
    assert_eq!(count, 3);
    assert_eq!(drained.len(), 3);
    assert!(drained.iter().all(|event| event.monotonic_nanos <= 2));

    consumer.clear_overflow();
    assert!(!consumer.ring_state().overflow);
    assert_eq!(
        consumer.ring_state().dropped_events,
        1,
        "drop counter is monotonic across overflow clears"
    );
}

#[test]
fn input_ring_preserves_producer_fifo_order() {
    let (mut producer, mut consumer) = RawInputRing::split_with_capacity(16);
    for i in 0..8 {
        producer.push_event(event(i));
    }
    let mut drained = Vec::new();
    let count = consumer.drain_up_to_nanos(u64::MAX, &mut drained);
    assert_eq!(count, 8);
    for (i, ev) in drained.iter().enumerate() {
        assert_eq!(ev.monotonic_nanos, i as u64);
    }
}

#[test]
fn runtime_input_ring_does_not_wrap_rtrb_halves_in_mutexes() {
    let source = include_str!("../src/platform/input_pump.rs");
    assert!(
        !source.contains("Mutex<Producer"),
        "RawInputProducer must own rtrb::Producer directly"
    );
    assert!(
        !source.contains("Mutex<Consumer"),
        "RawInputConsumer must own rtrb::Consumer directly"
    );
}

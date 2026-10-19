//! Regression coverage for backend events unknown to the Rust SDL bindings.
use sdl2::event::{Event, EventType};

// Reported by a user's SDL backend during startup. It is not a known SDL2
// EventType, but it must be ignored safely rather than transmuted into an enum.
const REPORTED_STARTUP_EVENT: u32 = 0x207;

fn raw_event(event_type: u32, timestamp: u32) -> sdl2::sys::SDL_Event {
    // SDL_Event is a C union of integer/pointer event payloads. Zero the whole
    // union so SDL_PushEvent can copy it without reading uninitialized padding.
    let mut raw: sdl2::sys::SDL_Event = unsafe { std::mem::zeroed() };
    raw.common = sdl2::sys::SDL_CommonEvent { type_: event_type, timestamp };
    raw
}

#[test]
fn unknown_backend_event_preserves_type_and_timestamp_without_aborting() {
    assert!(EventType::try_from(REPORTED_STARTUP_EVENT).is_err());
    assert_eq!(
        Event::from_ll(raw_event(REPORTED_STARTUP_EVENT, 123)),
        Event::Unknown { timestamp: 123, type_: REPORTED_STARTUP_EVENT },
    );
    assert_eq!(
        Event::from_ll(raw_event(sdl2::sys::SDL_EventType::SDL_QUIT as u32, 456)),
        Event::Quit { timestamp: 456 },
    );
}

#[test]
fn event_pump_continues_after_unknown_backend_event() {
    let sdl = sdl2::init().unwrap();
    let _events = sdl.event().unwrap();
    let mut pump = sdl.event_pump().unwrap();
    // Start with an empty queue so backend startup events cannot affect order.
    while pump.poll_event().is_some() {}
    let mut unknown = raw_event(REPORTED_STARTUP_EVENT, 0);
    // SDL copies this fully initialized event and retains no reference to it.
    assert_eq!(unsafe { sdl2::sys::SDL_PushEvent(&mut unknown) }, 1);
    sdl.event().unwrap().push_event(Event::Quit { timestamp: 0 }).unwrap();
    let events: Vec<_> = pump.poll_iter().collect();
    assert!(matches!(events.first(), Some(Event::Unknown { type_, .. }) if *type_ == REPORTED_STARTUP_EVENT));
    assert!(matches!(events.get(1), Some(Event::Quit { .. })));
}

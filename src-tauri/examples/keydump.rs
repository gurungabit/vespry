//! Debug harness: print what rdev delivers for a right-Cmd hold and tap.
//! Run: cargo run --example keydump   (needs Accessibility for the terminal)

use std::time::{Duration, Instant};

fn main() {
    let start = Instant::now();
    std::thread::spawn(move || {
        let result = rdev::listen(move |event| match event.event_type {
            rdev::EventType::KeyPress(k) => {
                println!("[{:6}ms] PRESS   {:?}", start.elapsed().as_millis(), k)
            }
            rdev::EventType::KeyRelease(k) => {
                println!("[{:6}ms] RELEASE {:?}", start.elapsed().as_millis(), k)
            }
            _ => {}
        });
        eprintln!("listen failed: {result:?}");
    });

    std::thread::sleep(Duration::from_millis(800));
    println!("--- simulating HOLD (down, 900ms, up) ---");
    let _ = rdev::simulate(&rdev::EventType::KeyPress(rdev::Key::MetaRight));
    std::thread::sleep(Duration::from_millis(900));
    let _ = rdev::simulate(&rdev::EventType::KeyRelease(rdev::Key::MetaRight));

    std::thread::sleep(Duration::from_millis(400));
    println!("--- simulating TAP (down, 60ms, up) ---");
    let _ = rdev::simulate(&rdev::EventType::KeyPress(rdev::Key::MetaRight));
    std::thread::sleep(Duration::from_millis(60));
    let _ = rdev::simulate(&rdev::EventType::KeyRelease(rdev::Key::MetaRight));

    std::thread::sleep(Duration::from_millis(600));
    println!("--- done ---");
}

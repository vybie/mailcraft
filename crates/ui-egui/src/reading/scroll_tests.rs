// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, TouchPhase, pos2, vec2};

fn distance(reading: bool) -> f32 {
    let mut window =
        egui_kittest::Harness::builder().with_accessibility_check(false).with_size(vec2(400.0, 300.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            move |ui, offset: &mut f32| {
                let area = if reading { super::body_scroll() } else { egui::ScrollArea::both().auto_shrink([false, false]) };
                let result = area.max_height(200.0).show(ui, |ui| ui.set_min_height(2_000.0));
                *offset = result.state.clamped_offset().y;
            },
            0.0,
        );
    window.run_steps(4);
    window.event(Event::PointerMoved(pos2(50.0, 50.0)));
    window.step();
    window.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, -40.0),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
        source: MouseWheelSource::Unknown,
    });
    window.run_steps(60);
    *window.state()
}

#[test]
fn reading_wheel_distance_matches_the_platform_multiplier() {
    let normal = distance(false);
    let reading = distance(true);
    assert!(normal > 0.0, "wheel did not scroll");
    let multiplier = if cfg!(target_os = "windows") { 2.0 } else { 1.0 };
    assert!((reading / normal - multiplier).abs() < 0.05, "reading={reading}, normal={normal}");
}

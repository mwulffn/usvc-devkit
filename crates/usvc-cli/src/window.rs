// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! SDL window: picture, sound, host keyboard and gamepads.
//!
//! The host keyboard becomes the console's USB keyboard. The first host
//! gamepad or joystick becomes the console's USB gamepad. F12 saves a
//! screenshot; closing the window ends the run.

use std::time::{Duration, Instant};

use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::controller::{Axis, Button, GameController};
use sdl2::event::Event;
use sdl2::joystick::{HatState, Joystick};
use sdl2::keyboard::Scancode;
use sdl2::pixels::PixelFormatEnum;
use usvc_core::keys::Key;
use usvc_core::{
    GamepadState, KeyboardState, AUDIO_HZ, CPU_HZ, CYCLES_PER_LINE, LINES_PER_FRAME, SCREEN_HEIGHT,
    SCREEN_WIDTH,
};

use crate::{write_png, AnyError, Args, Session};

/// The picture is 320x400 but is shown at the shape of a 640x400 mode.
const DISPLAY_WIDTH: u32 = 640;
const DISPLAY_HEIGHT: u32 = 400;
/// Queued audio above this is dropped so that sound cannot lag far behind.
const MAX_QUEUED_AUDIO_MS: u32 = 150;
/// If the host falls this far behind, stop trying to catch up.
const MAX_LAG: Duration = Duration::from_millis(100);
/// Stick movement below this (of 32768) counts as centred.
const STICK_DEADZONE: i32 = 8000;
const TRIGGER_THRESHOLD: i16 = 16000;
/// Rest position and range of the console's 8-bit gamepad axes.
const AXIS_CENTRE: i16 = 127;
const AXIS_MIN: i16 = 0;
const AXIS_MAX: i16 = 255;

// Button bits as the kernel names them (`GP_BUTTON_*`).
const GP_BUTTON_1: u32 = 1;
const GP_BUTTON_2: u32 = 2;
const GP_BUTTON_3: u32 = 4;
const GP_BUTTON_4: u32 = 8;
const GP_BUTTON_L1: u32 = 16;
const GP_BUTTON_R1: u32 = 32;
const GP_BUTTON_L2: u32 = 64;
const GP_BUTTON_R2: u32 = 128;
const GP_BUTTON_SELECT: u32 = 256;
const GP_BUTTON_START: u32 = 512;

const CONTROLLER_BUTTONS: [(Button, u32); 8] = [
    (Button::A, GP_BUTTON_1),
    (Button::B, GP_BUTTON_2),
    (Button::X, GP_BUTTON_3),
    (Button::Y, GP_BUTTON_4),
    (Button::LeftShoulder, GP_BUTTON_L1),
    (Button::RightShoulder, GP_BUTTON_R1),
    (Button::Back, GP_BUTTON_SELECT),
    (Button::Start, GP_BUTTON_START),
];

/// SDL scancodes are USB HID usage codes, which is what the console expects.
fn key_from_scancode(code: Scancode) -> Option<Key> {
    match code as i32 {
        c @ 4..=221 => Some(Key::Usage(c as u8)),
        c @ 224..=231 => Some(Key::Modifier(1 << (c - 224))),
        _ => None,
    }
}

/// Scale a signed 16-bit host axis to the console's 0..255 range.
fn scale_axis(v: i16) -> i16 {
    if (v as i32).abs() < STICK_DEADZONE {
        AXIS_CENTRE
    } else {
        ((v as i32 + 32768) >> 8) as i16
    }
}

/// Host devices currently plugged in, by SDL instance id.
#[derive(Default)]
struct Pads {
    controllers: Vec<GameController>,
    joysticks: Vec<Joystick>,
}

impl Pads {
    fn any(&self) -> bool {
        !self.controllers.is_empty() || !self.joysticks.is_empty()
    }

    /// State of the first device, in the console's terms.
    fn state(&self) -> GamepadState {
        let mut s = GamepadState {
            axes: [AXIS_CENTRE; 4],
            axis_min: AXIS_MIN,
            axis_max: AXIS_MAX,
            number_of_axes: 4,
            ..GamepadState::default()
        };
        let (mut left, mut right, mut up, mut down) = (false, false, false, false);
        if let Some(c) = self.controllers.first() {
            for (button, bit) in CONTROLLER_BUTTONS {
                if c.button(button) {
                    s.buttons |= bit;
                }
            }
            if c.axis(Axis::TriggerLeft) > TRIGGER_THRESHOLD {
                s.buttons |= GP_BUTTON_L2;
            }
            if c.axis(Axis::TriggerRight) > TRIGGER_THRESHOLD {
                s.buttons |= GP_BUTTON_R2;
            }
            let axes = [Axis::LeftX, Axis::LeftY, Axis::RightX, Axis::RightY];
            for (dst, axis) in s.axes.iter_mut().zip(axes) {
                *dst = scale_axis(c.axis(axis));
            }
            left = c.button(Button::DPadLeft);
            right = c.button(Button::DPadRight);
            up = c.button(Button::DPadUp);
            down = c.button(Button::DPadDown);
        } else if let Some(j) = self.joysticks.first() {
            // A plain HID joystick: buttons and axes in report order, as the
            // console's own driver would see them.
            for i in 0..j.num_buttons().min(32) {
                if j.button(i).unwrap_or(false) {
                    s.buttons |= 1 << i;
                }
            }
            for i in 0..j.num_axes().min(4) {
                s.axes[i as usize] = scale_axis(j.axis(i).unwrap_or(0));
            }
            if j.num_hats() > 0 {
                let hat = j.hat(0).unwrap_or(HatState::Centered);
                use HatState::*;
                left = matches!(hat, Left | LeftUp | LeftDown);
                right = matches!(hat, Right | RightUp | RightDown);
                up = matches!(hat, Up | LeftUp | RightUp);
                down = matches!(hat, Down | LeftDown | RightDown);
            }
            s.number_of_buttons = j.num_buttons().min(32) as u8;
        }
        // The direction pad moves the main axes to their limits.
        if left {
            s.axes[0] = AXIS_MIN;
        } else if right {
            s.axes[0] = AXIS_MAX;
        }
        if up {
            s.axes[1] = AXIS_MIN;
        } else if down {
            s.axes[1] = AXIS_MAX;
        }
        s
    }
}

pub fn run(session: &mut Session, args: &Args, title: &str) -> Result<(), AnyError> {
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let controller_sys = sdl.game_controller()?;
    let joystick_sys = sdl.joystick()?;
    let scale = args.scale.max(1);
    let window = video
        .window(
            &format!("uSVC - {title}"),
            DISPLAY_WIDTH * scale,
            DISPLAY_HEIGHT * scale,
        )
        .position_centered()
        .resizable()
        .build()?;
    let mut canvas = window.into_canvas().build()?;
    canvas.set_logical_size(DISPLAY_WIDTH, DISPLAY_HEIGHT)?;
    let creator = canvas.texture_creator();
    let mut texture = creator.create_texture_streaming(
        PixelFormatEnum::ARGB8888,
        SCREEN_WIDTH as u32,
        SCREEN_HEIGHT as u32,
    )?;

    let audio: Option<AudioQueue<i16>> = if args.mute {
        None
    } else {
        let spec = AudioSpecDesired {
            freq: Some(AUDIO_HZ as i32),
            channels: Some(1),
            samples: Some(1024),
        };
        let queue = sdl.audio()?.open_queue(None, &spec)?;
        queue.resume();
        Some(queue)
    };
    let max_queued_bytes = AUDIO_HZ * 2 * MAX_QUEUED_AUDIO_MS / 1000;

    let frame_time = Duration::from_nanos(1_000_000_000 * LINES_PER_FRAME * CYCLES_PER_LINE / CPU_HZ);
    let mut deadline = Instant::now() + frame_time;
    let mut events = sdl.event_pump()?;
    let mut keyboard = KeyboardState::default();
    let mut pads = Pads::default();
    let mut pixels = vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4];
    let mut screenshots = 0;

    'running: loop {
        let mut keys_changed = false;
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => break 'running,
                Event::KeyDown {
                    scancode: Some(Scancode::F12),
                    repeat: false,
                    ..
                } => {
                    screenshots += 1;
                    let path = args.png_dir.join(format!("screenshot{screenshots:03}.png"));
                    write_png(&path, session.m.framebuffer())?;
                    println!("saved {}", path.display());
                }
                Event::KeyDown {
                    scancode: Some(code),
                    repeat: false,
                    ..
                } => {
                    if let Some(key) = key_from_scancode(code) {
                        keyboard.press(key);
                        keys_changed = true;
                    }
                }
                Event::KeyUp {
                    scancode: Some(code),
                    ..
                } => {
                    if let Some(key) = key_from_scancode(code) {
                        keyboard.release(key);
                        keys_changed = true;
                    }
                }
                Event::ControllerDeviceAdded { which, .. } => {
                    let c = controller_sys.open(which)?;
                    println!("gamepad connected: {}", c.name());
                    pads.controllers.push(c);
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    pads.controllers.retain(|c| c.instance_id() != which);
                }
                Event::JoyDeviceAdded { which, .. } if !controller_sys.is_game_controller(which) => {
                    let j = joystick_sys.open(which)?;
                    println!("joystick connected: {}", j.name());
                    pads.joysticks.push(j);
                }
                Event::JoyDeviceRemoved { which, .. } => {
                    pads.joysticks.retain(|j| j.instance_id() != which);
                }
                _ => {}
            }
        }
        if keys_changed {
            session.m.set_keyboard(keyboard);
        }
        session
            .m
            .set_devices(!args.no_keyboard, args.gamepad || pads.any());
        if pads.any() {
            session.m.set_gamepad(pads.state());
        }

        let Some(samples) = session.run_frame()? else {
            break;
        };
        if let Some(queue) = &audio {
            if queue.size() > max_queued_bytes {
                queue.clear();
            }
            queue.queue_audio(&samples)?;
        }

        for (dst, px) in pixels.chunks_exact_mut(4).zip(session.m.framebuffer()) {
            dst.copy_from_slice(&(px | 0xFF00_0000).to_ne_bytes());
        }
        texture.update(None, &pixels, SCREEN_WIDTH * 4)?;
        canvas.clear();
        canvas.copy(&texture, None, None)?;
        canvas.present();

        if args.frames.is_some_and(|n| session.frame >= n) {
            break;
        }
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        } else if now - deadline > MAX_LAG {
            deadline = now;
        }
        deadline += frame_time;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scancodes_map_to_hid_usages_and_modifiers() {
        assert_eq!(key_from_scancode(Scancode::A), Some(Key::Usage(4)));
        assert_eq!(key_from_scancode(Scancode::Return), Some(Key::Usage(40)));
        assert_eq!(key_from_scancode(Scancode::Up), Some(Key::Usage(82)));
        assert_eq!(key_from_scancode(Scancode::LShift), Some(Key::Modifier(0x02)));
        assert_eq!(key_from_scancode(Scancode::RCtrl), Some(Key::Modifier(0x10)));
    }

    #[test]
    fn axis_scaling_reaches_both_limits_and_rests_at_centre() {
        assert_eq!(scale_axis(i16::MIN), AXIS_MIN);
        assert_eq!(scale_axis(i16::MAX), AXIS_MAX);
        assert_eq!(scale_axis(0), AXIS_CENTRE);
        assert_eq!(scale_axis(-3000), AXIS_CENTRE);
    }

    #[test]
    fn no_host_device_gives_a_centred_idle_pad() {
        let s = Pads::default().state();
        assert_eq!(s.buttons, 0);
        assert_eq!(s.axes, [AXIS_CENTRE; 4]);
    }
}

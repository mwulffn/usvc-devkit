//! Runs the shipped games. Needs the `reference/` submodules; tests are
//! skipped when they are not checked out.

use std::path::PathBuf;

use usvc_core::{FrameResult, Machine};

const WARMUP_FRAMES: u32 = 10;
const FRAMES: u32 = 40;
const VISIBLE_LINES: u64 = 400;

fn package(name: &str) -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../reference/uSVC/usc packages")
        .join(name);
    std::fs::read(path).ok()
}

fn hash(fb: &[u32]) -> u64 {
    fb.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, p| {
        (h ^ *p as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Boot a package, check the video timing invariants, return the frame hash.
fn run(name: &str, has_audio: bool) -> Option<u64> {
    let data = package(name)?;
    let mut m = Machine::new();
    m.load_usc(&data).expect("valid package");
    for _ in 0..WARMUP_FRAMES {
        m.run_frame();
    }
    m.take_stats();
    m.take_audio();
    for frame in 0..FRAMES {
        assert_eq!(m.run_frame(), FrameResult::Frame, "{name} frame {frame}");
    }
    let stats = m.take_stats();
    assert_eq!(stats.wfi_count, VISIBLE_LINES * FRAMES as u64, "{name}: lines drawn");
    assert!(stats.wfi_min_slack > 0, "{name}: scanline handler was late");
    // The mixer writes one sample per line; the sprite demos have no audio.
    let expected = if has_audio { 525 * FRAMES as usize } else { 0 };
    assert_eq!(m.take_audio().len(), expected, "{name}: audio samples");
    Some(hash(m.framebuffer()))
}

// Golden hashes are of frame 50. The pictures were approved by eye from
// screenshots, not compared with real hardware.
macro_rules! game_test {
    ($test:ident, $file:literal, $audio:literal, $golden:literal) => {
        #[test]
        fn $test() {
            let Some(first) = run($file, $audio) else {
                eprintln!("skipped: {} not found", $file);
                return;
            };
            assert_eq!(Some(first), run($file, $audio), "not deterministic");
            assert_eq!(first, $golden, "frame differs from the approved picture");
        }
    };
}

game_test!(tetris_bitmapped, "Tetris.usc", true, 0x01a80ca95e8ebff9);
game_test!(sprites8x8_tiles_8bpp, "Sprites8x8.usc", false, 0x2902a11182b5fa2d);
game_test!(sprites16x16_tiles_8bpp, "Sprites16x16.usc", false, 0x6c02b41e6554a3cd);
game_test!(horse_demo_tiles_4bpp, "HorseDemo.usc", true, 0x6305add40adc3673);
game_test!(redballs_tiles_4bpp, "RedBalls.usc", true, 0x8c24578c9bafae85);
game_test!(fairplay_race_tiles_8bpp, "FairPlayRace.usc", true, 0x0dd1370e40ed2681);

#[test]
fn tetris_menu_reacts_to_keys() {
    use usvc_core::keys::key_from_name;
    use usvc_core::KeyboardState;
    let Some(data) = package("Tetris.usc") else {
        return;
    };
    let run_with = |press: bool| {
        let mut m = Machine::new();
        m.load_usc(&data).unwrap();
        for frame in 0..200 {
            let mut kb = KeyboardState::default();
            if press && (100..103).contains(&frame) {
                kb.press(key_from_name("S").unwrap());
            }
            m.set_keyboard(kb);
            m.run_frame();
        }
        hash(m.framebuffer())
    };
    assert_ne!(run_with(false), run_with(true));
}

//! uSVC emulator front end: a window with sound and live input, or a
//! headless run that writes screenshots and a report.

use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use usvc_core::audio::DcBlocker;
use usvc_core::{Fault, FrameResult, Machine, AUDIO_HZ, GAME_BASE, SCREEN_HEIGHT, SCREEN_WIDTH};

mod input;
mod symbols;
#[cfg(feature = "window")]
mod window;

const DEFAULT_HEADLESS_FRAMES: u32 = 60;

#[derive(Parser)]
#[command(name = "usvc", about = "Run a uSVC game without hardware")]
struct Args {
    /// Game to run: a .usc package or a raw .bin linked at 0x6000
    game: PathBuf,
    /// Run without a window, as fast as possible, then print a summary
    #[arg(long)]
    headless: bool,
    /// Stop after this many frames (headless default: 60; window: unlimited)
    #[arg(short, long)]
    frames: Option<u32>,
    /// Write the last frame to this PNG file
    #[arg(long)]
    png: Option<PathBuf>,
    /// Write a PNG every N frames into --png-dir
    #[arg(long, value_name = "N")]
    png_every: Option<u32>,
    /// Directory for --png-every and for screenshots taken in the window
    #[arg(long, default_value = "out")]
    png_dir: PathBuf,
    /// Write the audio to this WAV file
    #[arg(long)]
    wav: Option<PathBuf>,
    /// Input script (see `input.rs` for the format)
    #[arg(long)]
    input: Option<PathBuf>,
    /// Tap a key at a frame, as FRAME:KEY (repeatable)
    #[arg(long, value_name = "FRAME:KEY")]
    tap: Vec<String>,
    /// Plug in a gamepad even if the host has none
    #[arg(long)]
    gamepad: bool,
    /// Unplug the keyboard, for games that prefer it over the gamepad
    #[arg(long)]
    no_keyboard: bool,
    /// Window size as a multiple of 640x400
    #[arg(long, default_value_t = 2)]
    scale: u32,
    /// No sound in the window
    #[arg(long)]
    mute: bool,
    /// Write a machine-readable run report (JSON) to this file
    #[arg(long)]
    report: Option<PathBuf>,
    /// Listing (.lss) to take symbol names from
    #[arg(long)]
    lss: Option<PathBuf>,
    /// Print video timing figures and stubbed registers
    #[arg(short, long)]
    verbose: bool,
}

type AnyError = Box<dyn std::error::Error>;

fn write_png(path: &Path, fb: &[u32]) -> Result<(), AnyError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let file = BufWriter::new(fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    let rgb: Vec<u8> = fb
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    writer.write_image_data(&rgb)?;
    Ok(())
}

fn write_wav(path: &Path, samples: &[i16]) -> std::io::Result<()> {
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&AUDIO_HZ.to_le_bytes());
    out.extend_from_slice(&(AUDIO_HZ * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    fs::write(path, out)
}

/// FNV-1a over the framebuffer, to compare frames between runs.
fn frame_hash(fb: &[u32]) -> u64 {
    fb.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, p| {
        (h ^ *p as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn print_fault(f: &Fault, syms: &symbols::Symbols) {
    eprintln!(
        "FAULT {:?} at pc={:#010x} ({}) addr={:#010x} lr={:#010x} ({}) sp={:#010x} cycle={}",
        f.kind,
        f.pc,
        syms.describe(f.pc),
        f.addr,
        f.lr,
        syms.describe(f.lr & !1),
        f.sp,
        f.cycle
    );
}

/// A running game plus everything collected from it, shared by both modes.
struct Session {
    m: Machine,
    script: input::Script,
    frame: u32,
    frames_done: u32,
    timeouts: u32,
    fault: Option<Fault>,
    /// Audio of the whole run, kept only when a WAV file was asked for.
    audio: Vec<i16>,
    keep_audio: bool,
    audio_samples: usize,
    dc: DcBlocker,
    png_every: Option<u32>,
    png_dir: PathBuf,
}

impl Session {
    /// Run one frame. Returns its audio, or `None` once the machine faulted.
    fn run_frame(&mut self) -> Result<Option<Vec<i16>>, AnyError> {
        self.script.apply(self.frame, &mut self.m);
        match self.m.run_frame() {
            FrameResult::Frame => self.frames_done += 1,
            FrameResult::Timeout => self.timeouts += 1,
            FrameResult::Fault(f) => {
                self.fault = Some(f);
                return Ok(None);
            }
        }
        self.frame += 1;
        let mut samples = self.m.take_audio();
        self.dc.process(&mut samples);
        self.audio_samples += samples.len();
        if self.keep_audio {
            self.audio.extend_from_slice(&samples);
        }
        let text = self.m.take_debug_output();
        if !text.is_empty() {
            print!("{}", String::from_utf8_lossy(&text));
        }
        if let Some(n) = self.png_every.filter(|n| *n > 0) {
            if self.frame % n == 0 {
                let path = self.png_dir.join(format!("frame{:05}.png", self.frame));
                write_png(&path, self.m.framebuffer())?;
            }
        }
        Ok(Some(samples))
    }

    fn run_headless(&mut self, frames: u32) -> Result<(), AnyError> {
        for _ in 0..frames {
            if self.run_frame()?.is_none() {
                break;
            }
        }
        Ok(())
    }
}

fn report_json(s: &Session, stats: &usvc_core::Stats, syms: &symbols::Symbols) -> String {
    let fault = match &s.fault {
        Some(f) => format!(
            "{{\"kind\": \"{:?}\", \"pc\": {}, \"symbol\": \"{}\", \"addr\": {}, \
             \"lr\": {}, \"sp\": {}, \"cycle\": {}}}",
            f.kind,
            f.pc,
            syms.describe(f.pc),
            f.addr,
            f.lr,
            f.sp,
            f.cycle
        ),
        None => "null".to_string(),
    };
    let min_slack = if stats.wfi_count == 0 {
        "null".to_string()
    } else {
        stats.wfi_min_slack.to_string()
    };
    format!(
        "{{\n  \"frames\": {},\n  \"timeouts\": {},\n  \"cycles\": {},\n  \
         \"instructions\": {},\n  \"frame_hash\": \"{:016x}\",\n  \"interrupts\": {},\n  \
         \"handler_cycles\": {},\n  \"lines_drawn\": {},\n  \"min_wfi_slack\": {min_slack},\n  \
         \"min_sp\": {},\n  \"audio_samples\": {},\n  \"pc\": {},\n  \"pc_symbol\": \"{}\",\n  \
         \"fault\": {fault}\n}}\n",
        s.frames_done,
        s.timeouts,
        s.m.cycles,
        stats.instructions,
        frame_hash(s.m.framebuffer()),
        stats.exceptions,
        stats.handler_cycles,
        stats.wfi_count,
        stats.min_sp,
        s.audio_samples,
        s.m.cpu.r[15],
        syms.describe(s.m.cpu.r[15]),
    )
}

fn print_summary(s: &Session, stats: &usvc_core::Stats, syms: &symbols::Symbols, verbose: bool) {
    let m = &s.m;
    println!(
        "frames={} timeouts={} cycles={} instructions={} hash={:016x}",
        s.frames_done,
        s.timeouts,
        m.cycles,
        stats.instructions,
        frame_hash(m.framebuffer())
    );
    let slack = if stats.wfi_count == 0 {
        "n/a".to_string()
    } else {
        stats.wfi_min_slack.to_string()
    };
    println!(
        "interrupts={} handler_share={:.1}% lines_drawn={} min_wfi_slack={slack} min_sp={:#010x}",
        stats.exceptions,
        100.0 * stats.handler_cycles as f64 / m.cycles.max(1) as f64,
        stats.wfi_count,
        stats.min_sp
    );
    println!("pc={:#010x} ({})", m.cpu.r[15], syms.describe(m.cpu.r[15]));
    if verbose {
        println!("video: {:?}", m.video_debug());
        let calls: Vec<String> = m
            .hle_call_counts()
            .iter()
            .enumerate()
            .filter(|(_, c)| **c > 0)
            .map(|(i, c)| format!("{i}:{c}"))
            .collect();
        println!("library calls (index:count): {}", calls.join(" "));
        let regs: Vec<String> = m
            .unmodelled_registers()
            .map(|r| format!("{r:#010x}"))
            .collect();
        println!("stubbed registers touched: {}", regs.join(" "));
    }
}

fn run(args: &Args) -> Result<ExitCode, AnyError> {
    let data = fs::read(&args.game).map_err(|e| format!("{}: {e}", args.game.display()))?;
    let syms = match &args.lss {
        Some(p) => {
            let text = fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
            symbols::Symbols::from_lss(&text)
        }
        None => symbols::Symbols::default(),
    };

    let mut m = Machine::new();
    let mut title = args
        .game
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if data.starts_with(b"USVC") {
        let pkg = m
            .load_usc(&data)
            .map_err(|e| format!("{}: {e}", args.game.display()))?;
        println!("loaded \"{}\" ({} bytes)", pkg.short_title, pkg.binary.len());
        if !pkg.short_title.is_empty() {
            title = pkg.short_title;
        }
    } else {
        m.load_bin(&data, GAME_BASE);
    }

    let mut script = match &args.input {
        Some(p) => {
            let text = fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            input::Script::parse(&text).map_err(|e| format!("{}: {e}", p.display()))?
        }
        None => input::Script::default(),
    };
    for spec in &args.tap {
        script.add_tap(spec).map_err(|e| format!("--tap: {e}"))?;
    }
    m.set_devices(!args.no_keyboard, args.gamepad || script.uses_gamepad);

    let mut session = Session {
        m,
        script,
        frame: 0,
        frames_done: 0,
        timeouts: 0,
        fault: None,
        audio: Vec::new(),
        keep_audio: args.wav.is_some(),
        audio_samples: 0,
        dc: DcBlocker::default(),
        png_every: args.png_every,
        png_dir: args.png_dir.clone(),
    };

    #[cfg(feature = "window")]
    if args.headless {
        session.run_headless(args.frames.unwrap_or(DEFAULT_HEADLESS_FRAMES))?;
    } else {
        window::run(&mut session, args, &title)?;
    }
    #[cfg(not(feature = "window"))]
    {
        let _ = &title;
        session.run_headless(args.frames.unwrap_or(DEFAULT_HEADLESS_FRAMES))?;
    }

    let stats = session.m.take_stats();
    print_summary(&session, &stats, &syms, args.verbose);
    if let Some(path) = &args.png {
        write_png(path, session.m.framebuffer())?;
    }
    if let Some(path) = &args.wav {
        write_wav(path, &session.audio)?;
    }
    if let Some(path) = &args.report {
        fs::write(path, report_json(&session, &stats, &syms))?;
    }
    Ok(match &session.fault {
        Some(f) => {
            print_fault(f, &syms);
            ExitCode::from(1)
        }
        None => ExitCode::SUCCESS,
    })
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

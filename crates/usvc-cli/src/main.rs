//! Headless runner: load a game, run frames, write screenshots and a report.

use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use usvc_core::{Fault, FrameResult, Machine, AUDIO_HZ, GAME_BASE, SCREEN_HEIGHT, SCREEN_WIDTH};

mod input;
mod symbols;

#[derive(Parser)]
#[command(name = "usvc", about = "Run a uSVC game without hardware")]
struct Args {
    /// Game to run: a .usc package or a raw .bin linked at 0x6000
    game: PathBuf,
    /// Number of frames to run
    #[arg(short, long, default_value_t = 60)]
    frames: u32,
    /// Write the last frame to this PNG file
    #[arg(long)]
    png: Option<PathBuf>,
    /// Write a PNG every N frames into --png-dir
    #[arg(long, value_name = "N")]
    png_every: Option<u32>,
    /// Directory for --png-every
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
    /// Plug in a gamepad as well as the keyboard
    #[arg(long)]
    gamepad: bool,
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

fn write_png(path: &Path, fb: &[u32]) -> Result<(), Box<dyn std::error::Error>> {
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

fn main() -> ExitCode {
    let args = Args::parse();
    let data = match fs::read(&args.game) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot read {}: {e}", args.game.display());
            return ExitCode::from(2);
        }
    };
    let syms = match &args.lss {
        Some(p) => match fs::read(p) {
            Ok(text) => symbols::Symbols::from_lss(&text),
            Err(e) => {
                eprintln!("cannot read {}: {e}", p.display());
                return ExitCode::from(2);
            }
        },
        None => symbols::Symbols::default(),
    };

    let mut m = Machine::new();
    if data.starts_with(b"USVC") {
        match m.load_usc(&data) {
            Ok(pkg) => println!("loaded \"{}\" ({} bytes)", pkg.short_title, pkg.binary.len()),
            Err(e) => {
                eprintln!("{}: {e}", args.game.display());
                return ExitCode::from(2);
            }
        }
    } else {
        m.load_bin(&data, GAME_BASE);
    }

    let mut script = match &args.input {
        Some(p) => match fs::read_to_string(p)
            .map_err(|e| e.to_string())
            .and_then(|t| input::Script::parse(&t))
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                return ExitCode::from(2);
            }
        },
        None => input::Script::default(),
    };
    for spec in &args.tap {
        if let Err(e) = script.add_tap(spec) {
            eprintln!("--tap: {e}");
            return ExitCode::from(2);
        }
    }
    m.set_devices(true, args.gamepad || script.uses_gamepad);

    let mut audio: Vec<i16> = Vec::new();
    let mut frames_done = 0;
    let mut timeouts = 0;
    let mut fault = None;
    for frame in 0..args.frames {
        script.apply(frame, &mut m);
        match m.run_frame() {
            FrameResult::Frame => frames_done += 1,
            FrameResult::Timeout => timeouts += 1,
            FrameResult::Fault(f) => {
                fault = Some(f);
                break;
            }
        }
        audio.extend(m.take_audio());
        let text = m.take_debug_output();
        if !text.is_empty() {
            print!("{}", String::from_utf8_lossy(&text));
        }
        if let Some(n) = args.png_every.filter(|n| *n > 0) {
            if (frame + 1) % n == 0 {
                let path = args.png_dir.join(format!("frame{:05}.png", frame + 1));
                if let Err(e) = write_png(&path, m.framebuffer()) {
                    eprintln!("cannot write {}: {e}", path.display());
                    return ExitCode::from(2);
                }
            }
        }
    }

    let stats = m.take_stats();
    println!(
        "frames={frames_done} timeouts={timeouts} cycles={} instructions={} hash={:016x}",
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
    if args.verbose {
        println!("video: {:?}", m.video_debug());
        let calls: Vec<String> = m
            .hle_call_counts()
            .iter()
            .enumerate()
            .filter(|(_, c)| **c > 0)
            .map(|(i, c)| format!("{i}:{c}"))
            .collect();
        println!("library calls (index:count): {}", calls.join(" "));
        let regs: Vec<String> = m.unmodelled_registers().map(|r| format!("{r:#010x}")).collect();
        println!("stubbed registers touched: {}", regs.join(" "));
    }
    if let Some(path) = &args.png {
        if let Err(e) = write_png(path, m.framebuffer()) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(2);
        }
    }
    if let Some(path) = &args.wav {
        if let Err(e) = write_wav(path, &audio) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(2);
        }
    }
    if let Some(path) = &args.report {
        let fault_json = match &fault {
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
        let report = format!(
            "{{\n  \"frames\": {frames_done},\n  \"timeouts\": {timeouts},\n  \
             \"cycles\": {},\n  \"instructions\": {},\n  \"frame_hash\": \"{:016x}\",\n  \
             \"interrupts\": {},\n  \"handler_cycles\": {},\n  \"lines_drawn\": {},\n  \
             \"min_wfi_slack\": {min_slack},\n  \"min_sp\": {},\n  \"audio_samples\": {},\n  \
             \"pc\": {},\n  \"pc_symbol\": \"{}\",\n  \"fault\": {fault_json}\n}}\n",
            m.cycles,
            stats.instructions,
            frame_hash(m.framebuffer()),
            stats.exceptions,
            stats.handler_cycles,
            stats.wfi_count,
            stats.min_sp,
            audio.len(),
            m.cpu.r[15],
            syms.describe(m.cpu.r[15]),
        );
        if let Err(e) = fs::write(path, report) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(2);
        }
    }
    match fault {
        Some(f) => {
            print_fault(&f, &syms);
            ExitCode::from(1)
        }
        None => ExitCode::SUCCESS,
    }
}

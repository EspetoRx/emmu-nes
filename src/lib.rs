use std::io::Cursor;
use tetanes_core::{
    apu::Apu,
    control_deck::{Clocked, Config, ControlDeck, HeadlessMode},
    input::{JoypadBtnState, Player},
};
use wasm_bindgen::prelude::*;

const AUDIO_SAMPLE_RATE: u32 = Apu::DEFAULT_SAMPLE_RATE as u32;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

fn nes_log(msg: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        log(msg);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = msg;
    }
}

fn log_rom_header_and_vectors(rom: &[u8]) {
    if rom.len() < 16 {
        nes_log(&format!(
            "[RomInspect] ROM muito curta: {} bytes",
            rom.len()
        ));
        return;
    }

    let magic = &rom[0..4];
    let prg_16k_banks = rom[4] as usize;
    let chr_8k_banks = rom[5] as usize;
    let flags6 = rom[6];
    let flags7 = rom[7];
    let mapper_ines = ((flags7 & 0xF0) | (flags6 >> 4)) as u16;
    let is_nes2 = (flags7 & 0x0C) == 0x08;
    let has_trainer = (flags6 & 0x04) != 0;
    let prg_start: usize = 16 + if has_trainer { 512 } else { 0 };
    let prg_len: usize = prg_16k_banks * 16 * 1024;
    let chr_start: usize = prg_start.saturating_add(prg_len);
    let chr_len: usize = chr_8k_banks * 8 * 1024;

    nes_log(&format!(
        "[RomInspect] magic={magic:02X?} nes2={is_nes2} mapper_ines={mapper_ines} prg16k={prg_16k_banks} ({prg_len}B) chr8k={chr_8k_banks} ({chr_len}B) f6={flags6:#04X} f7={flags7:#04X} total={}",
        rom.len()
    ));

    if prg_len > 0 && rom.len() >= prg_start + prg_len {
        let bank_size = 16 * 1024;
        for b in 0..prg_16k_banks {
            let start = prg_start + b * bank_size;
            let bank = &rom[start..start + bank_size];
            let lo = bank[0x3FFC] as u16;
            let hi = bank[0x3FFD] as u16;
            let rst = (hi << 8) | lo;
            let nonzero = bank.iter().filter(|&&x| x != 0).count();
            nes_log(&format!(
                "[RomInspect] PRG16K[{b}]: rst=${rst:04X} head={:02X?} end={:02X?} nonzero={nonzero}/{bank_size}",
                &bank[0..4],
                &bank[bank_size - 4..bank_size]
            ));
        }
    }

    if chr_len > 0 && rom.len() >= chr_start + chr_len {
        let chr_bank_size = 8 * 1024;
        for c in 0..chr_8k_banks {
            let start = chr_start + c * chr_bank_size;
            let bank = &rom[start..start + chr_bank_size];
            let nonzero = bank.iter().filter(|&&x| x != 0).count();
            if nonzero > 0 || c < 2 {
                nes_log(&format!(
                    "[RomInspect] CHR8K[{c}]: head={:02X?} nonzero={nonzero}/{chr_bank_size}",
                    &bank[0..4]
                ));
            }
        }
    } else if chr_8k_banks == 0 {
        nes_log("[RomInspect] CHR-ROM = 0 banks (usa CHR-RAM)");
    }
}

fn format_rom_load_error(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    let lower = message.to_lowercase();
    if lower.contains("mapper")
        && (lower.contains("unsupported") || lower.contains("unimplemented"))
    {
        return format!("Mapper não suportado neste núcleo NES: {message}");
    }
    message
}

pub struct NesCore {
    deck: ControlDeck,
    frame_counter: u32,
    last_frame_hash: u64,
}

impl NesCore {
    fn hash_frame(frame: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in frame.iter().step_by(16) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    pub fn new(rom_data: &[u8]) -> Result<Self, String> {
        nes_log(&format!("[NesCore::new] rom_bytes={}", rom_data.len()));
        log_rom_header_and_vectors(rom_data);

        let mut deck = ControlDeck::with_config(Config::default().with_sram_dir(None));
        deck.load_rom("game", &mut Cursor::new(rom_data))
            .map_err(format_rom_load_error)?;
        nes_log("[NesCore::new] m57 16K fixed-last-bank ativo ($C000->last)");

        deck.clear_audio_samples();
        Ok(Self {
            deck,
            frame_counter: 0,
            last_frame_hash: 0,
        })
    }

    pub fn run_frame(&mut self, render_graphics: bool) -> Result<Vec<u8>, String> {
        self.deck.set_headless_mode(if render_graphics {
            HeadlessMode::empty()
        } else {
            HeadlessMode::NO_VIDEO
        });

        loop {
            match self.deck.clock_frame().map_err(|error| error.to_string())? {
                Clocked::Continue => continue,
                Clocked::Complete => break,
                Clocked::Idle | Clocked::Stopped => {
                    return Err("O núcleo NES não avançou um frame.".to_owned());
                }
            }
        }

        self.frame_counter += 1;

        if render_graphics {
            let frame = self.deck.frame_buffer().to_vec();
            let h = Self::hash_frame(&frame);
            let changed = h != self.last_frame_hash;
            if self.frame_counter <= 5 || changed || self.frame_counter % 120 == 0 {
                let first_px = [frame[0], frame[1], frame[2], frame[3]];
                let mid_idx = (frame.len() / 2) & !3;
                let mid_px = [
                    frame[mid_idx],
                    frame[mid_idx + 1],
                    frame[mid_idx + 2],
                    frame[mid_idx + 3],
                ];
                let bus = self.deck.bus();
                let pc = bus.cpu.pc;
                let sp = bus.cpu.sp;
                let a = bus.cpu.acc;
                let x = bus.cpu.x;
                let y = bus.cpu.y;
                let open_bus = bus.ppu.open_bus;
                let b8000 = bus.memory.prg_peek(0x8000);
                let bc000 = bus.memory.prg_peek(0xC000);
                let rst_lo = bus.memory.prg_peek(0xFFFC);
                let rst_hi = bus.memory.prg_peek(0xFFFD);
                let rst_vec = (u16::from(rst_hi) << 8) | u16::from(rst_lo);
                let code_8000: [u8; 16] =
                    std::array::from_fn(|i| bus.memory.prg_peek(0x8000 + i as u16));
                nes_log(&format!(
                    "[NesCore::code8000] f={} code={code_8000:02X?}",
                    self.frame_counter
                ));

                let pc_window: [u8; 16] = std::array::from_fn(|i| {
                    let offset = i as i32 - 6;
                    let addr = (i32::from(pc) + offset).clamp(0x0000, 0xFFFF) as u16;
                    bus.peek(addr)
                });
                let stack_top: [u8; 8] = std::array::from_fn(|i| {
                    let sp_u16 = u16::from(sp.wrapping_add(1 + i as u8));
                    bus.peek(0x0100 | sp_u16)
                });
                nes_log(&format!(
                    "[NesCore::pc_dump] f={} pc=${pc:04X} (pc-6..pc+9)={pc_window:02X?} | sp=${sp:02X} stack={stack_top:02X?}",
                    self.frame_counter
                ));

                nes_log(&format!(
                    "[NesCore::run_frame] f={} changed={} hash={:#018X} rgba0={first_px:02X?} mid={mid_px:02X?} | pc=${pc:04X} A={a:02X} X={x:02X} Y={y:02X} ppu_bus={open_bus:#04X} $8000={b8000:02X} $C000={bc000:02X} rst=${rst_vec:04X}",
                    self.frame_counter, changed, h
                ));

                if self.frame_counter == 1 {
                    let fffc = bus.memory.prg_peek(0xFFFC);
                    let fffd = bus.memory.prg_peek(0xFFFD);
                    let ff00: [u8; 8] =
                        std::array::from_fn(|i| bus.memory.prg_peek(0xFF00 + i as u16));
                    nes_log(&format!(
                        "[NesCore::vecs] fffc_d=[{:02X}, {:02X}] (${:04X}) | ff00={ff00:02X?}",
                        fffc,
                        fffd,
                        (u16::from(fffd) << 8) | u16::from(fffc)
                    ));
                }
                if self.frame_counter == 1 {
                    let b830: [u8; 16] =
                        std::array::from_fn(|i| bus.memory.prg_peek(0xB830 + i as u16));
                    nes_log(&format!("[NesCore::b830] code={b830:02X?}"));
                }
                if self.frame_counter == 1 {
                    let b890: [u8; 16] =
                        std::array::from_fn(|i| bus.memory.prg_peek(0xB890 + i as u16));
                    let c492: [u8; 16] =
                        std::array::from_fn(|i| bus.memory.prg_peek(0xC492 + i as u16));
                    nes_log(&format!(
                        "[NesCore::b894] b890={b890:02X?} | c492={c492:02X?}"
                    ));
                }
                if self.frame_counter == 1 {
                    let ram0400: [u8; 16] =
                        std::array::from_fn(|i| bus.memory.prg_peek(0x0400 + i as u16));
                    let prg8518: [u8; 16] =
                        std::array::from_fn(|i| bus.memory.prg_peek(0x8518 + i as u16));
                    nes_log(&format!(
                        "[NesCore::trampoline] $0400={ram0400:02X?} | $8518={prg8518:02X?}"
                    ));
                }
                if self.frame_counter == 1 {
                    let b9618: [u8; 16] =
                        std::array::from_fn(|i| bus.memory.prg_peek(0x9618 + i as u16));
                    nes_log(&format!("[NesCore::loop961d] $9618={b9618:02X?}"));
                }

                let last_idx = frame.len().saturating_sub(4);
                let last_px = [
                    frame[last_idx],
                    frame[last_idx + 1],
                    frame[last_idx + 2],
                    frame[last_idx + 3],
                ];
                nes_log(&format!(
                    "[NesCore::px] f={} rgba0={first_px:02X?} mid={mid_px:02X?} last={last_px:02X?}",
                    self.frame_counter
                ));

                if self.frame_counter == 1 || self.frame_counter == 120 {
                    let ram0220: [u8; 32] =
                        std::array::from_fn(|i| bus.memory.prg_peek(0x0220 + i as u16));
                    nes_log(&format!(
                        "[NesCore::ram0220] f={} $0220..$023F={ram0220:02X?}",
                        self.frame_counter
                    ));
                }
                if self.frame_counter == 1 || self.frame_counter == 120 {
                    let ram0220: [u8; 32] = std::array::from_fn(|i| bus.peek(0x0220 + i as u16));
                    let ram0440: [u8; 32] = std::array::from_fn(|i| bus.peek(0x0440 + i as u16));
                    nes_log(&format!(
                        "[NesCore::cpu_ram] f={} pc=${pc:04X} $0220..$023F={ram0220:02X?} | $0440..$045F={ram0440:02X?}",
                        self.frame_counter
                    ));
                }

                for line in tetanes_core::mapper::m057_mapper57::take_m057_logs() {
                    nes_log(&line);
                }
                for line in tetanes_core::mapper::m083_mapper83::take_m83_logs() {
                    nes_log(&line);
                }
            }
            self.last_frame_hash = h;
            Ok(frame)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        let samples = self.deck.audio_samples().to_vec();
        self.deck.clear_audio_samples();
        if (self.frame_counter <= 5 || self.frame_counter % 60 == 0) && !samples.is_empty() {
            let max_abs = samples.iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()));
            nes_log(&format!(
                "[NesCore::audio] f={} count={} max_abs={max_abs:.4}",
                self.frame_counter,
                samples.len()
            ));
        }
        samples
    }

    pub fn set_button(&mut self, button: &str, pressed: bool) -> Result<(), String> {
        let button = match button {
            "A" => JoypadBtnState::A,
            "B" => JoypadBtnState::B,
            "Start" => JoypadBtnState::START,
            "Select" => JoypadBtnState::SELECT,
            "Up" => JoypadBtnState::UP,
            "Down" => JoypadBtnState::DOWN,
            "Left" => JoypadBtnState::LEFT,
            "Right" => JoypadBtnState::RIGHT,
            "X" | "Y" | "L" | "R" => return Ok(()),
            _ => return Err(format!("Botão de controle NES desconhecido: {button}")),
        };
        self.deck
            .joypad_mut(Player::One)
            .set_button(button, pressed);
        Ok(())
    }

    pub fn save_state(&mut self) -> Result<Vec<u8>, String> {
        let mut state = Vec::new();
        self.deck
            .save_state(&mut state)
            .map_err(|error| format!("Não foi possível salvar o estado do NES: {error}"))?;
        Ok(state)
    }

    pub fn load_state(&mut self, state: &[u8]) -> Result<(), String> {
        self.deck
            .load_state(Cursor::new(state))
            .map_err(|error| format!("Não foi possível carregar o estado do NES: {error}"))?;
        self.deck.clear_audio_samples();
        Ok(())
    }
}

#[wasm_bindgen]
pub struct NesEmulator {
    core: NesCore,
}

#[wasm_bindgen]
impl NesEmulator {
    #[wasm_bindgen(constructor)]
    pub fn new(rom_data: &[u8]) -> Result<NesEmulator, JsValue> {
        let core = NesCore::new(rom_data).map_err(|error| JsValue::from_str(&error))?;
        Ok(Self { core })
    }

    #[wasm_bindgen(js_name = runFrame)]
    pub fn run_frame(&mut self, render_graphics: bool) -> Result<Vec<u8>, JsValue> {
        self.core
            .run_frame(render_graphics)
            .map_err(|error| JsValue::from_str(&error))
    }

    #[wasm_bindgen(js_name = audioSamples)]
    pub fn audio_samples(&mut self) -> Vec<f32> {
        self.core.take_audio_samples()
    }

    #[wasm_bindgen(js_name = audioSampleRate)]
    pub fn audio_sample_rate(&self) -> u32 {
        AUDIO_SAMPLE_RATE
    }

    #[wasm_bindgen(js_name = audioChannels)]
    pub fn audio_channels(&self) -> u32 {
        1
    }

    #[wasm_bindgen(js_name = setButton)]
    pub fn set_button(&mut self, button: &str, pressed: bool) -> Result<(), JsValue> {
        self.core
            .set_button(button, pressed)
            .map_err(|error| JsValue::from_str(&error))
    }

    #[wasm_bindgen(js_name = saveState)]
    pub fn save_state(&mut self) -> Result<Vec<u8>, JsValue> {
        self.core
            .save_state()
            .map_err(|error| JsValue::from_str(&error))
    }

    #[wasm_bindgen(js_name = loadState)]
    pub fn load_state(&mut self, state: &[u8]) -> Result<(), JsValue> {
        self.core
            .load_state(state)
            .map_err(|error| JsValue::from_str(&error))
    }
}

#[cfg(test)]
mod tests {
    use super::{AUDIO_SAMPLE_RATE, NesCore};
    use tetanes_core::{cart::Cart, memory::RamState, ppu::Mirroring, video::Frame};

    fn test_rom() -> Vec<u8> {
        let mut rom = vec![0; 16 + 16 * 1024 + 8 * 1024];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        rom[5] = 1;
        let program = [
            0xa9, 0x01, 0x8d, 0x15, 0x40, 0xa9, 0xbf, 0x8d, 0x00, 0x40, 0xa9, 0xfd, 0x8d, 0x02,
            0x40, 0xa9, 0x08, 0x8d, 0x03, 0x40, 0x4c, 0x14, 0x80,
        ];
        rom[16..16 + program.len()].copy_from_slice(&program);
        rom[16 + 0x3ffc] = 0x00;
        rom[16 + 0x3ffd] = 0x80;
        rom
    }

    fn mapper_227_rom() -> Vec<u8> {
        const BANK_SIZE: usize = 16 * 1024;
        const BANK_COUNT: usize = 64;

        let mut rom = vec![0; 16 + BANK_SIZE * BANK_COUNT];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = BANK_COUNT as u8;
        rom[6] = 0x30;
        rom[7] = 0xE0;

        for (bank, bytes) in rom[16..].chunks_mut(BANK_SIZE).enumerate() {
            bytes.fill(bank as u8);
        }
        rom[16..19].copy_from_slice(&[0x4C, 0x00, 0x80]);
        rom[16 + BANK_SIZE - 4] = 0x00;
        rom[16 + BANK_SIZE - 3] = 0x80;
        rom
    }

    fn mapper_227_cart() -> Cart {
        let mut rom = std::io::Cursor::new(mapper_227_rom());
        Cart::from_rom("mapper 227 test", &mut rom, RamState::Random)
            .expect("mapper 227 test ROM should load")
    }

    #[test]
    fn mapper_227_decodes_address_bits_and_rebuilds_banks() {
        let mut cart = mapper_227_cart();
        assert_eq!(cart.mapper.mirroring(), Mirroring::Vertical);

        cart.mapper.write_register(&mut cart.memory, 0x800C, 0xFF);
        assert_eq!(cart.memory.prg_peek(0x8010), 3);
        assert_eq!(cart.memory.prg_peek(0xC010), 0);

        cart.mapper.write_register(&mut cart.memory, 0x8015, 0);
        assert_eq!(cart.memory.prg_peek(0x8010), 4);
        assert_eq!(cart.memory.prg_peek(0xC010), 0);

        cart.mapper.write_register(&mut cart.memory, 0x8100, 0);
        assert_eq!(cart.memory.prg_peek(0x8010), 32);
        assert_eq!(cart.memory.prg_peek(0xC010), 32);

        cart.mapper.write_register(&mut cart.memory, 0x8200, 0);
        assert_eq!(cart.memory.prg_peek(0x8010), 0);
        assert_eq!(cart.memory.prg_peek(0xC010), 7);

        cart.mapper.write_register(&mut cart.memory, 0x8094, 0);
        assert_eq!(cart.memory.prg_peek(0x8010), 5);
        assert_eq!(cart.memory.prg_peek(0xC010), 5);

        cart.mapper.write_register(&mut cart.memory, 0x8097, 0);
        assert_eq!(cart.memory.prg_peek(0x8010), 4);
        assert_eq!(cart.memory.prg_peek(0xC010), 5);
        assert_eq!(cart.mapper.mirroring(), Mirroring::Horizontal);

        cart.memory.unmap_prg(0x8000, 0x8000);
        cart.mapper.update_banks(&mut cart.memory);
        assert_eq!(cart.memory.prg_peek(0x8010), 4);
        assert_eq!(cart.memory.prg_peek(0xC010), 5);
    }

    #[test]
    fn mapper_227_save_state_restores() {
        let mut nes = NesCore::new(&mapper_227_rom()).expect("mapper 227 ROM should load");
        nes.run_frame(false).expect("frame should execute");
        let state = nes.save_state().expect("state should save");
        nes.run_frame(false).expect("second frame should execute");
        nes.load_state(&state).expect("state should load");

        assert_eq!(
            nes.run_frame(true)
                .expect("restored frame should execute")
                .len(),
            Frame::SIZE
        );
    }

    #[test]
    fn mapper_227_rom_runs_a_frame() {
        let mut nes = NesCore::new(&mapper_227_rom()).expect("mapper 227 ROM should load");
        assert_eq!(
            nes.run_frame(true)
                .expect("mapper 227 frame should execute")
                .len(),
            Frame::SIZE
        );
    }

    #[test]
    fn rejects_invalid_and_truncated_roms() {
        assert!(NesCore::new(b"not a rom").is_err());

        let mut truncated = test_rom();
        truncated.truncate(20);
        assert!(NesCore::new(&truncated).is_err());
    }

    #[test]
    fn accepts_nes_2_roms() {
        let mut rom = test_rom();
        rom[7] = 0x08;
        assert!(NesCore::new(&rom).is_ok());
    }

    #[test]
    fn executes_frame_and_produces_native_rate_audio() {
        let mut nes = NesCore::new(&test_rom()).expect("test ROM should load");
        let frame = nes.run_frame(true).expect("frame should execute");
        let audio = nes.take_audio_samples();

        assert_eq!(frame.len(), Frame::SIZE);
        assert_eq!(Frame::SIZE, 256 * 240 * 4);
        assert!(
            (700..850).contains(&audio.len()),
            "unexpected frame audio length: {}",
            audio.len()
        );
        assert!(audio.iter().any(|sample| sample.abs() > 0.0001));
        assert_eq!(AUDIO_SAMPLE_RATE, 48_000);
    }

    #[test]
    fn skips_video_rendering_without_skipping_audio_or_later_frames() {
        let mut nes = NesCore::new(&test_rom()).expect("test ROM should load");
        let skipped_frame = nes.run_frame(false).expect("frame should execute");
        let audio = nes.take_audio_samples();
        let visible_frame = nes.run_frame(true).expect("frame should execute");

        assert!(skipped_frame.is_empty());
        assert!(!audio.is_empty());
        assert_eq!(visible_frame.len(), Frame::SIZE);
    }

    #[test]
    fn save_state_restores_emulation() {
        let mut nes = NesCore::new(&test_rom()).expect("test ROM should load");
        nes.run_frame(false).expect("frame should execute");
        let state = nes.save_state().expect("state should save");
        nes.run_frame(false).expect("second frame should execute");
        nes.load_state(&state).expect("state should load");
        assert!(nes.run_frame(true).is_ok());
    }
}

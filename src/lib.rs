use std::io::Cursor;
use tetanes_core::{
    apu::Apu,
    control_deck::{Clocked, Config, ControlDeck, HeadlessMode},
    input::{JoypadBtnState, Player},
};
use wasm_bindgen::prelude::*;

const AUDIO_SAMPLE_RATE: u32 = Apu::DEFAULT_SAMPLE_RATE as u32;

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
}

impl NesCore {
    pub fn new(rom_data: &[u8]) -> Result<Self, String> {
        let mut deck = ControlDeck::with_config(Config::default().with_sram_dir(None));
        deck.load_rom("game", &mut Cursor::new(rom_data))
            .map_err(format_rom_load_error)?;

        deck.clear_audio_samples();
        Ok(Self { deck })
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

        if render_graphics {
            Ok(self.deck.frame_buffer().to_vec())
        } else {
            Ok(Vec::new())
        }
    }

    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        let samples = self.deck.audio_samples().to_vec();
        self.deck.clear_audio_samples();
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

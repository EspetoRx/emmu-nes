# emmu-nes

Wrapper WebAssembly do Emmu para **NES / Famicom**. O núcleo de emulação é o [TetaNES Core](https://github.com/lukexor/tetanes), vendorizado como submódulo em `../tetanes-core` e patchado no workspace pela raiz `Cargo.toml`. No desktop, NES roda no Mesen via `emmu-mesen-sys`; este crate é usado no navegador.

## API

### Nativa (`NesCore`)

- `new(rom_data)` — carrega iNES 1.0 ou NES 2.0 usando `ControlDeck::with_config(Config::default().with_sram_dir(None))`.
- `run_frame(render_graphics)` — sempre clocka CPU/PPU/APU; quando `render_graphics` é `false`, usa `HeadlessMode::NO_VIDEO` e continua gerando áudio.
- `take_audio_samples() -> Vec<f32>` — amostras mono em 48 kHz (`Apu::DEFAULT_SAMPLE_RATE`).
- `set_button(button, pressed)` — `A`, `B`, `Start`, `Select`, `Up`, `Down`, `Left`, `Right`. `X`, `Y`, `L` e `R` são aceitos como no-op para manter compatibilidade com a UI; nomes desconhecidos retornam erro.
- `save_state()` / `load_state(bytes)` — estado completo do console serializado pelo TetaNES.
- Apenas o jogador 1 é controlado por esta ponte.

### WASM (`NesEmulator`)

Exportado com `wasm-bindgen`: `new`, `runFrame(renderGraphics)`, `audioSamples`, `audioSampleRate`, `audioChannels` (`1`), `setButton`, `saveState` e `loadState`.

Erros de mapper unsupported/unimplemented são reescritos em português por `format_rom_load_error`, para aparecerem de forma legível na UI.

## Estrutura de um frame

- Resolução: `256 × 240`.
- Formato: RGBA8888 (`Frame::SIZE = 256 * 240 * 4`).
- Áudio: 48.000 Hz, mono, `f32`.

## Árvore de arquivos

```text
emmu-nes/
├── .gitignore        # /target
├── Cargo.toml        # cdylib + rlib; tetanes-core, wasm-bindgen, getrandom (wasm)
├── LICENSE           # MIT, Copyright (c) 2026 EspetoRx
├── examples/         # reservado para exemplos
└── src/
    ├── lib.rs        # NesCore, NesEmulator WASM, formatação de erros, testes
    ├── apu.rs        # arquivo vazio, não declarado como módulo
    ├── cpu.rs        # implementação 6502 avulsa, não compilada
    ├── mapper.rs     # mappers avulsos, não compilados
    ├── memory.rs     # barramento/iNES avulso, não compilado
    └── ppu.rs        # stub de PPU avulso, não compilado
```

> Os arquivos `cpu.rs`, `mapper.rs`, `memory.rs`, `ppu.rs` e `apu.rs` existem no histórico do submódulo, mas **não são declarados em `lib.rs`**. O crate compilado usa somente `tetanes-core`.

## Build, teste e WASM

A partir da raiz do workspace:

```sh
cargo test -p emmu-nes
```

Com ROM real:

```sh
EMMU_NES_TEST_ROM="/caminho/para/jogo.nes" cargo test -p emmu-nes -- --nocapture
```

Pacote WASM usado pela UI:

```sh
cd crates/emmu-nes
wasm-pack build --target web \
  --out-dir ../../platforms/web/ui/src/wasm/nes \
  --out-name emmu_nes
```

Esse comando já faz parte de `npm run build:wasm` em `platforms/web/ui`.

## Testes incluídos

- decodificação de banco/save state do mapper 227;
- rejeição de ROM truncada/inválida;
- aceitação de cabeçalho NES 2.0;
- tamanho correto do frame;
- áudio preservado quando o vídeo é desligado;
- round-trip de save state;
- taxa de amostragem de 48 kHz.

## Licenças

| Componente | Licença | Crédito |
| --- | --- | --- |
| `emmu-nes` | MIT | `Copyright (c) 2026 EspetoRx` — ver `LICENSE` |
| `tetanes-core` | MIT OR Apache-2.0 | `Copyright (c) 2021 Luke Petherbridge <me@lukeworks.tech>` — ver `../tetanes-core/LICENSE-MIT` e `LICENSE-APACHE` |
| `wasm-bindgen` | MIT OR Apache-2.0 | rustwasm |
| `getrandom` (wasm) | MIT OR Apache-2.0 | rust-random |

Nenhuma ROM é distribuída. Suporte a mappers depende do TetaNES; iNES 1.0, NES 2.0 e multicarts com mapper 227 (por exemplo, 1200-in-1) são suportados.

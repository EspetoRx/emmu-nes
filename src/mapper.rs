#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreenLower,
    SingleScreenUpper,
}

pub trait Mapper {
    fn read_prg(&self, addr: u16) -> u8;
    fn write_prg(&mut self, addr: u16, data: u8);
    fn read_chr(&self, addr: u16) -> u8;
    fn write_chr(&mut self, addr: u16, data: u8);
    fn mirroring(&self) -> Mirroring;
    fn step_scanline(&mut self) {}
    fn irq_pending(&self) -> bool {
        false
    }
}

/// Mapper 0 (NROM): PRG de 16 KB (espelhado em 0xC000) ou 32 KB, e CHR-ROM/RAM de 8 KB.
pub struct NromMapper {
    prg_rom: Vec<u8>,
    chr_data: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
}

impl NromMapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr_data = if chr_is_ram {
            vec![0; 8192]
        } else {
            chr_rom
        };
        Self {
            prg_rom,
            chr_data,
            chr_is_ram,
            mirroring,
        }
    }

    fn map_prg_index(&self, addr: u16) -> usize {
        if self.prg_rom.is_empty() {
            return 0;
        }
        let offset = (addr - 0x8000) as usize;
        if self.prg_rom.len() <= 0x4000 {
            offset % self.prg_rom.len()
        } else {
            offset % self.prg_rom.len()
        }
    }
}

impl Mapper for NromMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let idx = self.map_prg_index(addr);
                self.prg_rom.get(idx).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, _addr: u16, _data: u8) {}

    fn read_chr(&self, addr: u16) -> u8 {
        if self.chr_data.is_empty() {
            return 0;
        }
        let idx = (addr as usize) % self.chr_data.len();
        self.chr_data[idx]
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        if !self.chr_is_ram || self.chr_data.is_empty() {
            return;
        }
        let idx = (addr as usize) % self.chr_data.len();
        self.chr_data[idx] = data;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Mapper 1 (MMC1 / SxROM): registrador serial de 5 bits em 0x8000..=0xFFFF.
pub struct Mmc1Mapper {
    prg_rom: Vec<u8>,
    chr_data: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: Vec<u8>,
    mirroring: Mirroring,

    shift_reg: u8,
    write_count: u8,
    control: u8,
    chr_bank_0: u8,
    chr_bank_1: u8,
    prg_bank: u8,
}

impl Mmc1Mapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr_data = if chr_is_ram {
            vec![0; 8192]
        } else {
            chr_rom
        };
        Self {
            prg_rom,
            chr_data,
            chr_is_ram,
            prg_ram: vec![0; 8192],
            mirroring,
            shift_reg: 0x10,
            write_count: 0,
            control: 0x0C,
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
        }
    }

    fn apply_mirroring_from_control(&mut self) {
        self.mirroring = match self.control & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            _ => Mirroring::Horizontal,
        };
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x9FFF => {
                self.control = value;
                self.apply_mirroring_from_control();
            }
            0xA000..=0xBFFF => {
                self.chr_bank_0 = value;
            }
            0xC000..=0xDFFF => {
                self.chr_bank_1 = value;
            }
            0xE000..=0xFFFF => {
                self.prg_bank = value;
            }
            _ => {}
        }
    }

    fn total_prg_16k_banks(&self) -> usize {
        (self.prg_rom.len() / 0x4000).max(1)
    }

    fn total_chr_4k_banks(&self) -> usize {
        (self.chr_data.len() / 0x1000).max(1)
    }

    fn read_prg_16k(&self, bank: usize, offset_in_bank: usize) -> u8 {
        if self.prg_rom.is_empty() {
            return 0;
        }
        let wrapped_bank = bank % self.total_prg_16k_banks();
        let idx = wrapped_bank * 0x4000 + (offset_in_bank & 0x3FFF);
        self.prg_rom.get(idx).copied().unwrap_or(0)
    }

    fn read_chr_4k(&self, bank: usize, offset_in_bank: usize) -> u8 {
        if self.chr_data.is_empty() {
            return 0;
        }
        let wrapped_bank = bank % self.total_chr_4k_banks();
        let idx = wrapped_bank * 0x1000 + (offset_in_bank & 0x0FFF);
        self.chr_data.get(idx).copied().unwrap_or(0)
    }

    fn write_chr_4k(&mut self, bank: usize, offset_in_bank: usize, data: u8) {
        if !self.chr_is_ram || self.chr_data.is_empty() {
            return;
        }
        let wrapped_bank = bank % self.total_chr_4k_banks();
        let idx = wrapped_bank * 0x1000 + (offset_in_bank & 0x0FFF);
        if let Some(slot) = self.chr_data.get_mut(idx) {
            *slot = data;
        }
    }
}

impl Mapper for Mmc1Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let idx = (addr - 0x6000) as usize;
                self.prg_ram.get(idx).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => {
                let prg_mode = (self.control >> 2) & 0x03;
                let bank_select = (self.prg_bank & 0x0F) as usize;
                let last_bank = self.total_prg_16k_banks().saturating_sub(1);
                let offset = (addr & 0x3FFF) as usize;

                match prg_mode {
                    0 | 1 => {
                        let base_bank = (bank_select & !0x01) + ((addr as usize >> 14) & 0x01);
                        self.read_prg_16k(base_bank, offset)
                    }
                    2 => {
                        if addr < 0xC000 {
                            self.read_prg_16k(0, offset)
                        } else {
                            self.read_prg_16k(bank_select, offset)
                        }
                    }
                    _ => {
                        if addr < 0xC000 {
                            self.read_prg_16k(bank_select, offset)
                        } else {
                            self.read_prg_16k(last_bank, offset)
                        }
                    }
                }
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF => {
                let prg_ram_disabled = (self.prg_bank & 0x10) != 0;
                if !prg_ram_disabled {
                    let idx = (addr - 0x6000) as usize;
                    if let Some(slot) = self.prg_ram.get_mut(idx) {
                        *slot = data;
                    }
                }
            }
            0x8000..=0xFFFF => {
                if (data & 0x80) != 0 {
                    self.shift_reg = 0x10;
                    self.write_count = 0;
                    self.control |= 0x0C;
                    self.apply_mirroring_from_control();
                    return;
                }

                self.shift_reg >>= 1;
                self.shift_reg |= (data & 0x01) << 4;
                self.write_count += 1;

                if self.write_count == 5 {
                    let completed = self.shift_reg;
                    self.shift_reg = 0x10;
                    self.write_count = 0;
                    self.write_register(addr, completed);
                }
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let chr_mode = (self.control & 0x10) != 0;
        let offset = (addr & 0x0FFF) as usize;
        if !chr_mode {
            let base = ((self.chr_bank_0 & !0x01) as usize) + ((addr as usize >> 12) & 0x01);
            self.read_chr_4k(base, offset)
        } else if addr < 0x1000 {
            self.read_chr_4k(self.chr_bank_0 as usize, offset)
        } else {
            self.read_chr_4k(self.chr_bank_1 as usize, offset)
        }
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        let chr_mode = (self.control & 0x10) != 0;
        let offset = (addr & 0x0FFF) as usize;
        if !chr_mode {
            let base = ((self.chr_bank_0 & !0x01) as usize) + ((addr as usize >> 12) & 0x01);
            self.write_chr_4k(base, offset, data);
        } else if addr < 0x1000 {
            self.write_chr_4k(self.chr_bank_0 as usize, offset, data);
        } else {
            self.write_chr_4k(self.chr_bank_1 as usize, offset, data);
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Mapper 2 (UxROM): banco chaveável de 16 KB em 0x8000..=0xBFFF e último banco fixo em 0xC000..=0xFFFF.
pub struct UxRomMapper {
    prg_rom: Vec<u8>,
    chr_data: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    selected_bank: usize,
}

impl UxRomMapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr_data = if chr_is_ram {
            vec![0; 8192]
        } else {
            chr_rom
        };
        Self {
            prg_rom,
            chr_data,
            chr_is_ram,
            mirroring,
            selected_bank: 0,
        }
    }

    fn total_16k_banks(&self) -> usize {
        (self.prg_rom.len() / 0x4000).max(1)
    }
}

impl Mapper for UxRomMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        if self.prg_rom.is_empty() {
            return 0;
        }
        let bank_count = self.total_16k_banks();
        let last_bank = bank_count.saturating_sub(1);
        let offset = (addr & 0x3FFF) as usize;

        match addr {
            0x8000..=0xBFFF => {
                let bank = self.selected_bank % bank_count;
                self.prg_rom
                    .get(bank * 0x4000 + offset)
                    .copied()
                    .unwrap_or(0)
            }
            0xC000..=0xFFFF => self
                .prg_rom
                .get(last_bank * 0x4000 + offset)
                .copied()
                .unwrap_or(0),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, data: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.selected_bank = data as usize;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        if self.chr_data.is_empty() {
            return 0;
        }
        let idx = (addr as usize) % self.chr_data.len();
        self.chr_data[idx]
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        if !self.chr_is_ram || self.chr_data.is_empty() {
            return;
        }
        let idx = (addr as usize) % self.chr_data.len();
        self.chr_data[idx] = data;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Mapper 3 (CNROM): chaveia bancos de CHR de 8 KB via escrita em 0x8000..=0xFFFF.
pub struct CnRomMapper {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    chr_bank: usize,
}

impl CnRomMapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            chr_bank: 0,
        }
    }

    fn total_chr_8k_banks(&self) -> usize {
        (self.chr_rom.len() / 0x2000).max(1)
    }
}

impl Mapper for CnRomMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        if self.prg_rom.is_empty() {
            return 0;
        }
        match addr {
            0x8000..=0xFFFF => {
                let idx = ((addr - 0x8000) as usize) % self.prg_rom.len();
                self.prg_rom[idx]
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, data: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.chr_bank = (data & 0x03) as usize;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() {
            return 0;
        }
        let bank = self.chr_bank % self.total_chr_8k_banks();
        let idx = bank * 0x2000 + ((addr & 0x1FFF) as usize);
        self.chr_rom.get(idx).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, _addr: u16, _data: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Implementação reutilizável do MMC3 (Mapper 4 e base do Mapper 37).
pub struct Mmc3 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,

    bank_select: u8,
    bank_registers: [u8; 8],
    prg_ram_protect: u8,

    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq_requested: bool,

    prg_base_bank: usize,
    prg_bank_mask: usize,
    chr_base_bank: usize,
    chr_bank_mask: usize,
}

impl Mmc3 {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr_data = if chr_is_ram {
            vec![0; 8192]
        } else {
            chr_rom
        };

        let prg_8k_count = (prg_rom.len() / 0x2000).max(1);
        let chr_1k_count = (chr_data.len() / 0x0400).max(1);

        Self {
            prg_rom,
            chr_rom: chr_data,
            prg_ram: vec![0; 8192],
            chr_is_ram,
            mirroring,
            bank_select: 0,
            bank_registers: [0; 8],
            prg_ram_protect: 0x80,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_requested: false,
            prg_base_bank: 0,
            prg_bank_mask: prg_8k_count.saturating_sub(1),
            chr_base_bank: 0,
            chr_bank_mask: chr_1k_count.saturating_sub(1),
        }
    }

    pub fn set_outer_banks(
        &mut self,
        prg_base_bank: usize,
        prg_bank_mask: usize,
        chr_base_bank: usize,
        chr_bank_mask: usize,
    ) {
        self.prg_base_bank = prg_base_bank;
        self.prg_bank_mask = prg_bank_mask;
        self.chr_base_bank = chr_base_bank;
        self.chr_bank_mask = chr_bank_mask;
    }

    fn total_prg_8k_banks(&self) -> usize {
        (self.prg_rom.len() / 0x2000).max(1)
    }

    fn total_chr_1k_banks(&self) -> usize {
        (self.chr_rom.len() / 0x0400).max(1)
    }

    fn resolve_prg_bank(&self, raw_bank: usize) -> usize {
        let total = self.total_prg_8k_banks();
        let masked = (raw_bank & self.prg_bank_mask) + self.prg_base_bank;
        masked % total
    }

    fn resolve_chr_bank(&self, raw_bank: usize) -> usize {
        let total = self.total_chr_1k_banks();
        let masked = (raw_bank & self.chr_bank_mask) + self.chr_base_bank;
        masked % total
    }

    fn prg_mode(&self) -> bool {
        (self.bank_select & 0x40) != 0
    }

    fn chr_inversion(&self) -> bool {
        (self.bank_select & 0x80) != 0
    }

    fn prg_ram_enabled(&self) -> bool {
        (self.prg_ram_protect & 0x80) != 0
    }

    fn prg_ram_write_protected(&self) -> bool {
        (self.prg_ram_protect & 0x40) != 0
    }

    fn map_prg_8k_bank(&self, addr: u16) -> usize {
        let r6 = self.bank_registers[6] as usize;
        let r7 = self.bank_registers[7] as usize;
        let second_last = self.prg_bank_mask.saturating_sub(1);
        let last = self.prg_bank_mask;

        let raw = match (addr, self.prg_mode()) {
            (0x8000..=0x9FFF, false) => r6,
            (0x8000..=0x9FFF, true) => second_last,
            (0xA000..=0xBFFF, _) => r7,
            (0xC000..=0xDFFF, false) => second_last,
            (0xC000..=0xDFFF, true) => r6,
            (0xE000..=0xFFFF, _) => last,
            _ => 0,
        };

        self.resolve_prg_bank(raw)
    }

    fn map_chr_1k_bank(&self, addr: u16) -> usize {
        let r0 = (self.bank_registers[0] & !0x01) as usize;
        let r1 = (self.bank_registers[1] & !0x01) as usize;
        let r2 = self.bank_registers[2] as usize;
        let r3 = self.bank_registers[3] as usize;
        let r4 = self.bank_registers[4] as usize;
        let r5 = self.bank_registers[5] as usize;

        let raw = match (addr, self.chr_inversion()) {
            (0x0000..=0x03FF, false) => r0,
            (0x0400..=0x07FF, false) => r0 + 1,
            (0x0800..=0x0BFF, false) => r1,
            (0x0C00..=0x0FFF, false) => r1 + 1,
            (0x1000..=0x13FF, false) => r2,
            (0x1400..=0x17FF, false) => r3,
            (0x1800..=0x1BFF, false) => r4,
            (0x1C00..=0x1FFF, false) => r5,

            (0x0000..=0x03FF, true) => r2,
            (0x0400..=0x07FF, true) => r3,
            (0x0800..=0x0BFF, true) => r4,
            (0x0C00..=0x0FFF, true) => r5,
            (0x1000..=0x13FF, true) => r0,
            (0x1400..=0x17FF, true) => r0 + 1,
            (0x1800..=0x1BFF, true) => r1,
            (0x1C00..=0x1FFF, true) => r1 + 1,

            _ => 0,
        };

        self.resolve_chr_bank(raw)
    }

    pub fn read_prg_internal(&self, addr: u16) -> u8 {
        if self.prg_rom.is_empty() {
            return 0;
        }
        let bank = self.map_prg_8k_bank(addr);
        let offset = (addr & 0x1FFF) as usize;
        let index = bank * 0x2000 + offset;
        self.prg_rom.get(index).copied().unwrap_or(0)
    }

    pub fn write_prg_internal(&mut self, addr: u16, data: u8) {
        let even = (addr & 0x0001) == 0;
        match addr {
            0x8000..=0x9FFF => {
                if even {
                    self.bank_select = data;
                } else {
                    let reg_idx = (self.bank_select & 0x07) as usize;
                    self.bank_registers[reg_idx] = data;
                }
            }
            0xA000..=0xBFFF => {
                if even {
                    self.mirroring = if (data & 0x01) == 0 {
                        Mirroring::Vertical
                    } else {
                        Mirroring::Horizontal
                    };
                } else {
                    self.prg_ram_protect = data;
                }
            }
            0xC000..=0xDFFF => {
                if even {
                    self.irq_latch = data;
                } else {
                    self.irq_reload = true;
                }
            }
            0xE000..=0xFFFF => {
                if even {
                    self.irq_enabled = false;
                    self.irq_requested = false;
                } else {
                    self.irq_enabled = true;
                }
            }
            _ => {}
        }
    }
}

impl Mapper for Mmc3 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled() {
                    let idx = (addr - 0x6000) as usize;
                    self.prg_ram.get(idx).copied().unwrap_or(0)
                } else {
                    0
                }
            }
            0x8000..=0xFFFF => self.read_prg_internal(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled() && !self.prg_ram_write_protected() {
                    let idx = (addr - 0x6000) as usize;
                    if let Some(slot) = self.prg_ram.get_mut(idx) {
                        *slot = data;
                    }
                }
            }
            0x8000..=0xFFFF => self.write_prg_internal(addr, data),
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank = self.map_chr_1k_bank(addr);
        let offset = (addr & 0x03FF) as usize;
        let index = bank * 0x0400 + offset;
        self.chr_rom.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        if !self.chr_is_ram {
            return;
        }
        let bank = self.map_chr_1k_bank(addr);
        let offset = (addr & 0x03FF) as usize;
        let index = bank * 0x0400 + offset;
        if let Some(slot) = self.chr_rom.get_mut(index) {
            *slot = data;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn step_scanline(&mut self) {
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter -= 1;
        }

        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_requested = true;
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_requested
    }
}

/// Mapper 37: Multicartucho NES baseado em MMC3 com registrador externo em 0x6000..=0x7FFF.
pub struct Mapper37 {
    mmc3: Mmc3,
    outer_reg: u8,
}

impl Mapper37 {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        let mut mapper = Self {
            mmc3: Mmc3::new(prg_rom, chr_rom, mirroring),
            outer_reg: 0,
        };
        mapper.apply_outer_bank();
        mapper
    }

    fn apply_outer_bank(&mut self) {
        let (prg_base, prg_mask, chr_base, chr_mask) = match self.outer_reg & 0x03 {
            0 => (0, 0x0F, 0, 0x7F),
            1 => (16, 0x0F, 0, 0x7F),
            2 => (32, 0x1F, 128, 0x7F),
            _ => (32, 0x0F, 128, 0x7F),
        };

        self.mmc3
            .set_outer_banks(prg_base, prg_mask, chr_base, chr_mask);
    }
}

impl Mapper for Mapper37 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => 0,
            0x8000..=0xFFFF => self.mmc3.read_prg_internal(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if (data & 0x04) != 0 {
                    self.outer_reg = data & 0x03;
                    self.apply_outer_bank();
                }
            }
            0x8000..=0xFFFF => self.mmc3.write_prg_internal(addr, data),
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        self.mmc3.write_chr(addr, data)
    }

    fn mirroring(&self) -> Mirroring {
        self.mmc3.mirroring()
    }

    fn step_scanline(&mut self) {
        self.mmc3.step_scanline();
    }

    fn irq_pending(&self) -> bool {
        self.mmc3.irq_pending()
    }
}

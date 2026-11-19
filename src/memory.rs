use crate::mapper::{
    CnRomMapper, Mapper, Mapper37, Mirroring, Mmc1Mapper, Mmc3, NromMapper, UxRomMapper,
};

pub struct Bus {
    pub cpu_ram: [u8; 2048], // 2KB de RAM interna do NES
    pub mapper_id: u8,
    pub mapper: Option<Box<dyn Mapper>>,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            cpu_ram: [0; 2048],
            mapper_id: 0,
            mapper: None,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                // Espelhamento da RAM interna de 2KB
                self.cpu_ram[(addr & 0x07FF) as usize]
            }
            0x6000..=0xFFFF => self
                .mapper
                .as_ref()
                .map(|mapper| mapper.read_prg(addr))
                .unwrap_or(0),
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, data: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.cpu_ram[(addr & 0x07FF) as usize] = data;
            }
            0x6000..=0xFFFF => {
                if let Some(mapper) = self.mapper.as_mut() {
                    mapper.write_prg(addr, data);
                }
            }
            _ => {}
        }
    }

    pub fn read_chr(&self, addr: u16) -> u8 {
        self.mapper
            .as_ref()
            .map(|mapper| mapper.read_chr(addr))
            .unwrap_or(0)
    }

    pub fn write_chr(&mut self, addr: u16, data: u8) {
        if let Some(mapper) = self.mapper.as_mut() {
            mapper.write_chr(addr, data);
        }
    }

    pub fn mirroring(&self) -> Mirroring {
        self.mapper
            .as_ref()
            .map(|mapper| mapper.mirroring())
            .unwrap_or(Mirroring::Horizontal)
    }

    pub fn load_rom(&mut self, rom_data: &[u8]) -> Result<(), String> {
        if rom_data.len() < 16 || &rom_data[0..4] != &[b'N', b'E', b'S', 0x1A] {
            return Err("Arquivo ROM inválido ou formato iNES não reconhecido.".to_string());
        }

        let prg_rom_blocks = rom_data[4] as usize;
        let chr_rom_blocks = rom_data[5] as usize;
        let flags_6 = rom_data[6];
        let flags_7 = rom_data[7];

        self.mapper_id = (flags_7 & 0xF0) | (flags_6 >> 4);

        let mirroring = if (flags_6 & 0x08) != 0 {
            Mirroring::Vertical
        } else if (flags_6 & 0x01) != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        let has_trainer = (flags_6 & 0x04) != 0;
        let header_size = 16;
        let trainer_size = if has_trainer { 512 } else { 0 };

        let prg_start = header_size + trainer_size;
        let prg_size = prg_rom_blocks * 16384;
        let chr_start = prg_start + prg_size;
        let chr_size = chr_rom_blocks * 8192;

        if rom_data.len() < chr_start + chr_size {
            return Err(
                "Arquivo ROM truncado: tamanho menor que o declarado no cabeçalho iNES."
                    .to_string(),
            );
        }

        let prg_rom = rom_data[prg_start..(prg_start + prg_size)].to_vec();
        let chr_rom = if chr_rom_blocks > 0 {
            rom_data[chr_start..(chr_start + chr_size)].to_vec()
        } else {
            Vec::new()
        };

        let mapper: Box<dyn Mapper> = match self.mapper_id {
            0 => Box::new(NromMapper::new(prg_rom, chr_rom, mirroring)),
            1 => Box::new(Mmc1Mapper::new(prg_rom, chr_rom, mirroring)),
            2 => Box::new(UxRomMapper::new(prg_rom, chr_rom, mirroring)),
            3 => Box::new(CnRomMapper::new(prg_rom, chr_rom, mirroring)),
            4 => Box::new(Mmc3::new(prg_rom, chr_rom, mirroring)),
            37 => Box::new(Mapper37::new(prg_rom, chr_rom, mirroring)),
            other => {
                return Err(format!("unimplemented mapper {other}"));
            }
        };

        self.mapper = Some(mapper);
        Ok(())
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

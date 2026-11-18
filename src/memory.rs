pub struct Bus {
    pub cpu_ram: [u8; 2048], // 2KB de RAM interna do NES
}

impl Bus {
    pub fn new() -> Self {
        Self {
            cpu_ram: [0; 2048],
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                // Espelhamento da RAM de 2KB (endereços 0x0000-0x07FF espelhados até 0x1FFF)
                self.cpu_ram[(addr & 0x07FF) as usize]
            }
            _ => 0, // Outros barramentos (PPU, APU, Cartucho) virão depois
        }
    }

    pub fn write(&mut self, addr: u16, data: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.cpu_ram[(addr & 0x07FF) as usize] = data;
            }
            _ => {}
        }
    }

    pub fn load_rom(&mut self, _rom_data: &[u8]) {
        // Implementação do parser do iNES virá aqui
    }
}

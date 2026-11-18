pub struct Ppu2C02 {
    pub framebuffer: [u32; 256 * 240], // Resolução nativa do NES: 256x240 pixels
}

impl Ppu2C02 {
    pub fn new() -> Self {
        Self {
            framebuffer: [0; 256 * 240],
        }
    }

    pub fn reset(&mut self) {
        // Limpa a tela ou reseta registradores da PPU
        self.framebuffer.fill(0);
    }

    pub fn step(&mut self) {
        // A lógica de renderização de scanlines da PPU entrará aqui futuramente
    }

    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }
}

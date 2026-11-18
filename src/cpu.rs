use crate::memory::Bus;

const CARRY: u8 = 0x01;
const ZERO: u8 = 0x02;
const INTERRUPT_DISABLE: u8 = 0x04;
const DECIMAL: u8 = 0x08;
const BREAK: u8 = 0x10;
const UNUSED: u8 = 0x20;
const OVERFLOW: u8 = 0x40;
const NEGATIVE: u8 = 0x80;

#[derive(Clone, Copy)]
enum AddressMode {
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    IndexedIndirect,
    IndirectIndexed,
}

#[derive(Clone, Copy)]
enum ReadOperation {
    Adc,
    And,
    Bit,
    CompareA,
    CompareX,
    CompareY,
    Eor,
    LoadA,
    LoadX,
    LoadY,
    Ora,
    Sbc,
}

#[derive(Clone, Copy)]
enum StoreRegister {
    A,
    X,
    Y,
}

#[derive(Clone, Copy)]
enum ModifyOperation {
    Asl,
    Lsr,
    Rol,
    Ror,
    Inc,
    Dec,
}

pub struct Cpu6502 {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub status: u8,
    pub pc: u16,
}

impl Cpu6502 {
    pub fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xfd,
            status: INTERRUPT_DISABLE | UNUSED,
            pc: 0,
        }
    }

    pub fn reset(&mut self, bus: &mut Bus) {
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.sp = 0xfd;
        self.status = INTERRUPT_DISABLE | UNUSED;
        self.pc = Self::read_word(bus, 0xfffc);
    }

    pub fn step(&mut self, bus: &mut Bus) -> u8 {
        let opcode = self.fetch_byte(bus);

        if let Some((operation, mode)) = Self::read_instruction(opcode) {
            let (value, cycles) = self.read_operand(bus, mode);
            self.execute_read(operation, value);
            return cycles;
        }

        if let Some((register, mode)) = Self::store_instruction(opcode) {
            let address = self.address(bus, mode).0;
            let value = match register {
                StoreRegister::A => self.a,
                StoreRegister::X => self.x,
                StoreRegister::Y => self.y,
            };
            bus.write(address, value);
            return Self::store_cycles(opcode);
        }

        if let Some((operation, mode)) = Self::modify_instruction(opcode) {
            if let Some(mode) = mode {
                let address = self.address(bus, mode).0;
                let value = bus.read(address);
                let result = self.modify(operation, value);
                bus.write(address, result);
            } else {
                self.a = self.modify(operation, self.a);
            }
            return Self::modify_cycles(opcode);
        }

        match opcode {
            0x00 => {
                self.pc = self.pc.wrapping_add(1);
                self.push_word(bus, self.pc);
                self.push(bus, self.status | BREAK | UNUSED);
                self.set_flag(INTERRUPT_DISABLE, true);
                self.pc = Self::read_word(bus, 0xfffe);
                7
            }
            0x08 => {
                self.push(bus, self.status | BREAK | UNUSED);
                3
            }
            0x10 => self.branch(bus, !self.flag(NEGATIVE)),
            0x18 => {
                self.set_flag(CARRY, false);
                2
            }
            0x20 => {
                let target = self.fetch_word(bus);
                self.push_word(bus, self.pc.wrapping_sub(1));
                self.pc = target;
                6
            }
            0x28 => {
                self.status = (self.pop(bus) & !BREAK) | UNUSED;
                4
            }
            0x30 => self.branch(bus, self.flag(NEGATIVE)),
            0x38 => {
                self.set_flag(CARRY, true);
                2
            }
            0x40 => {
                self.status = (self.pop(bus) & !BREAK) | UNUSED;
                self.pc = self.pop_word(bus);
                6
            }
            0x48 => {
                self.push(bus, self.a);
                3
            }
            0x4c => {
                self.pc = self.fetch_word(bus);
                3
            }
            0x50 => self.branch(bus, !self.flag(OVERFLOW)),
            0x58 => {
                self.set_flag(INTERRUPT_DISABLE, false);
                2
            }
            0x60 => {
                self.pc = self.pop_word(bus).wrapping_add(1);
                6
            }
            0x68 => {
                self.a = self.pop(bus);
                self.set_zero_negative(self.a);
                4
            }
            0x6c => {
                let pointer = self.fetch_word(bus);
                let low = bus.read(pointer);
                let high_address = (pointer & 0xff00) | u16::from((pointer as u8).wrapping_add(1));
                self.pc = u16::from_le_bytes([low, bus.read(high_address)]);
                5
            }
            0x70 => self.branch(bus, self.flag(OVERFLOW)),
            0x78 => {
                self.set_flag(INTERRUPT_DISABLE, true);
                2
            }
            0x88 => {
                self.y = self.y.wrapping_sub(1);
                self.set_zero_negative(self.y);
                2
            }
            0x8a => {
                self.a = self.x;
                self.set_zero_negative(self.a);
                2
            }
            0x90 => self.branch(bus, !self.flag(CARRY)),
            0x98 => {
                self.a = self.y;
                self.set_zero_negative(self.a);
                2
            }
            0x9a => {
                self.sp = self.x;
                2
            }
            0xa8 => {
                self.y = self.a;
                self.set_zero_negative(self.y);
                2
            }
            0xaa => {
                self.x = self.a;
                self.set_zero_negative(self.x);
                2
            }
            0xb0 => self.branch(bus, self.flag(CARRY)),
            0xb8 => {
                self.set_flag(OVERFLOW, false);
                2
            }
            0xba => {
                self.x = self.sp;
                self.set_zero_negative(self.x);
                2
            }
            0xc8 => {
                self.y = self.y.wrapping_add(1);
                self.set_zero_negative(self.y);
                2
            }
            0xca => {
                self.x = self.x.wrapping_sub(1);
                self.set_zero_negative(self.x);
                2
            }
            0xd0 => self.branch(bus, !self.flag(ZERO)),
            0xd8 => {
                self.set_flag(DECIMAL, false);
                2
            }
            0xe8 => {
                self.x = self.x.wrapping_add(1);
                self.set_zero_negative(self.x);
                2
            }
            0xea => 2,
            0xf0 => self.branch(bus, self.flag(ZERO)),
            0xf8 => {
                self.set_flag(DECIMAL, true);
                2
            }
            _ => panic!(
                "Unsupported 6502 opcode {opcode:#04x} at {:#06x}",
                self.pc.wrapping_sub(1)
            ),
        }
    }

    fn read_instruction(opcode: u8) -> Option<(ReadOperation, AddressMode)> {
        let instruction = match opcode {
            0x69 => (ReadOperation::Adc, AddressMode::Immediate),
            0x65 => (ReadOperation::Adc, AddressMode::ZeroPage),
            0x75 => (ReadOperation::Adc, AddressMode::ZeroPageX),
            0x6d => (ReadOperation::Adc, AddressMode::Absolute),
            0x7d => (ReadOperation::Adc, AddressMode::AbsoluteX),
            0x79 => (ReadOperation::Adc, AddressMode::AbsoluteY),
            0x61 => (ReadOperation::Adc, AddressMode::IndexedIndirect),
            0x71 => (ReadOperation::Adc, AddressMode::IndirectIndexed),
            0x29 => (ReadOperation::And, AddressMode::Immediate),
            0x25 => (ReadOperation::And, AddressMode::ZeroPage),
            0x35 => (ReadOperation::And, AddressMode::ZeroPageX),
            0x2d => (ReadOperation::And, AddressMode::Absolute),
            0x3d => (ReadOperation::And, AddressMode::AbsoluteX),
            0x39 => (ReadOperation::And, AddressMode::AbsoluteY),
            0x21 => (ReadOperation::And, AddressMode::IndexedIndirect),
            0x31 => (ReadOperation::And, AddressMode::IndirectIndexed),
            0x24 => (ReadOperation::Bit, AddressMode::ZeroPage),
            0x2c => (ReadOperation::Bit, AddressMode::Absolute),
            0xc9 => (ReadOperation::CompareA, AddressMode::Immediate),
            0xc5 => (ReadOperation::CompareA, AddressMode::ZeroPage),
            0xd5 => (ReadOperation::CompareA, AddressMode::ZeroPageX),
            0xcd => (ReadOperation::CompareA, AddressMode::Absolute),
            0xdd => (ReadOperation::CompareA, AddressMode::AbsoluteX),
            0xd9 => (ReadOperation::CompareA, AddressMode::AbsoluteY),
            0xc1 => (ReadOperation::CompareA, AddressMode::IndexedIndirect),
            0xd1 => (ReadOperation::CompareA, AddressMode::IndirectIndexed),
            0xe0 => (ReadOperation::CompareX, AddressMode::Immediate),
            0xe4 => (ReadOperation::CompareX, AddressMode::ZeroPage),
            0xec => (ReadOperation::CompareX, AddressMode::Absolute),
            0xc0 => (ReadOperation::CompareY, AddressMode::Immediate),
            0xc4 => (ReadOperation::CompareY, AddressMode::ZeroPage),
            0xcc => (ReadOperation::CompareY, AddressMode::Absolute),
            0x49 => (ReadOperation::Eor, AddressMode::Immediate),
            0x45 => (ReadOperation::Eor, AddressMode::ZeroPage),
            0x55 => (ReadOperation::Eor, AddressMode::ZeroPageX),
            0x4d => (ReadOperation::Eor, AddressMode::Absolute),
            0x5d => (ReadOperation::Eor, AddressMode::AbsoluteX),
            0x59 => (ReadOperation::Eor, AddressMode::AbsoluteY),
            0x41 => (ReadOperation::Eor, AddressMode::IndexedIndirect),
            0x51 => (ReadOperation::Eor, AddressMode::IndirectIndexed),
            0xa9 => (ReadOperation::LoadA, AddressMode::Immediate),
            0xa5 => (ReadOperation::LoadA, AddressMode::ZeroPage),
            0xb5 => (ReadOperation::LoadA, AddressMode::ZeroPageX),
            0xad => (ReadOperation::LoadA, AddressMode::Absolute),
            0xbd => (ReadOperation::LoadA, AddressMode::AbsoluteX),
            0xb9 => (ReadOperation::LoadA, AddressMode::AbsoluteY),
            0xa1 => (ReadOperation::LoadA, AddressMode::IndexedIndirect),
            0xb1 => (ReadOperation::LoadA, AddressMode::IndirectIndexed),
            0xa2 => (ReadOperation::LoadX, AddressMode::Immediate),
            0xa6 => (ReadOperation::LoadX, AddressMode::ZeroPage),
            0xb6 => (ReadOperation::LoadX, AddressMode::ZeroPageY),
            0xae => (ReadOperation::LoadX, AddressMode::Absolute),
            0xbe => (ReadOperation::LoadX, AddressMode::AbsoluteY),
            0xa0 => (ReadOperation::LoadY, AddressMode::Immediate),
            0xa4 => (ReadOperation::LoadY, AddressMode::ZeroPage),
            0xb4 => (ReadOperation::LoadY, AddressMode::ZeroPageX),
            0xac => (ReadOperation::LoadY, AddressMode::Absolute),
            0xbc => (ReadOperation::LoadY, AddressMode::AbsoluteX),
            0x09 => (ReadOperation::Ora, AddressMode::Immediate),
            0x05 => (ReadOperation::Ora, AddressMode::ZeroPage),
            0x15 => (ReadOperation::Ora, AddressMode::ZeroPageX),
            0x0d => (ReadOperation::Ora, AddressMode::Absolute),
            0x1d => (ReadOperation::Ora, AddressMode::AbsoluteX),
            0x19 => (ReadOperation::Ora, AddressMode::AbsoluteY),
            0x01 => (ReadOperation::Ora, AddressMode::IndexedIndirect),
            0x11 => (ReadOperation::Ora, AddressMode::IndirectIndexed),
            0xe9 | 0xeb => (ReadOperation::Sbc, AddressMode::Immediate),
            0xe5 => (ReadOperation::Sbc, AddressMode::ZeroPage),
            0xf5 => (ReadOperation::Sbc, AddressMode::ZeroPageX),
            0xed => (ReadOperation::Sbc, AddressMode::Absolute),
            0xfd => (ReadOperation::Sbc, AddressMode::AbsoluteX),
            0xf9 => (ReadOperation::Sbc, AddressMode::AbsoluteY),
            0xe1 => (ReadOperation::Sbc, AddressMode::IndexedIndirect),
            0xf1 => (ReadOperation::Sbc, AddressMode::IndirectIndexed),
            _ => return None,
        };
        Some(instruction)
    }

    fn modify_instruction(opcode: u8) -> Option<(ModifyOperation, Option<AddressMode>)> {
        let instruction = match opcode {
            0x0a => (ModifyOperation::Asl, None),
            0x06 => (ModifyOperation::Asl, Some(AddressMode::ZeroPage)),
            0x16 => (ModifyOperation::Asl, Some(AddressMode::ZeroPageX)),
            0x0e => (ModifyOperation::Asl, Some(AddressMode::Absolute)),
            0x1e => (ModifyOperation::Asl, Some(AddressMode::AbsoluteX)),
            0x4a => (ModifyOperation::Lsr, None),
            0x46 => (ModifyOperation::Lsr, Some(AddressMode::ZeroPage)),
            0x56 => (ModifyOperation::Lsr, Some(AddressMode::ZeroPageX)),
            0x4e => (ModifyOperation::Lsr, Some(AddressMode::Absolute)),
            0x5e => (ModifyOperation::Lsr, Some(AddressMode::AbsoluteX)),
            0x2a => (ModifyOperation::Rol, None),
            0x26 => (ModifyOperation::Rol, Some(AddressMode::ZeroPage)),
            0x36 => (ModifyOperation::Rol, Some(AddressMode::ZeroPageX)),
            0x2e => (ModifyOperation::Rol, Some(AddressMode::Absolute)),
            0x3e => (ModifyOperation::Rol, Some(AddressMode::AbsoluteX)),
            0x6a => (ModifyOperation::Ror, None),
            0x66 => (ModifyOperation::Ror, Some(AddressMode::ZeroPage)),
            0x76 => (ModifyOperation::Ror, Some(AddressMode::ZeroPageX)),
            0x6e => (ModifyOperation::Ror, Some(AddressMode::Absolute)),
            0x7e => (ModifyOperation::Ror, Some(AddressMode::AbsoluteX)),
            0xe6 => (ModifyOperation::Inc, Some(AddressMode::ZeroPage)),
            0xf6 => (ModifyOperation::Inc, Some(AddressMode::ZeroPageX)),
            0xee => (ModifyOperation::Inc, Some(AddressMode::Absolute)),
            0xfe => (ModifyOperation::Inc, Some(AddressMode::AbsoluteX)),
            0xc6 => (ModifyOperation::Dec, Some(AddressMode::ZeroPage)),
            0xd6 => (ModifyOperation::Dec, Some(AddressMode::ZeroPageX)),
            0xce => (ModifyOperation::Dec, Some(AddressMode::Absolute)),
            0xde => (ModifyOperation::Dec, Some(AddressMode::AbsoluteX)),
            _ => return None,
        };
        Some(instruction)
    }

    fn store_instruction(opcode: u8) -> Option<(StoreRegister, AddressMode)> {
        let instruction = match opcode {
            0x85 => (StoreRegister::A, AddressMode::ZeroPage),
            0x95 => (StoreRegister::A, AddressMode::ZeroPageX),
            0x8d => (StoreRegister::A, AddressMode::Absolute),
            0x9d => (StoreRegister::A, AddressMode::AbsoluteX),
            0x99 => (StoreRegister::A, AddressMode::AbsoluteY),
            0x81 => (StoreRegister::A, AddressMode::IndexedIndirect),
            0x91 => (StoreRegister::A, AddressMode::IndirectIndexed),
            0x86 => (StoreRegister::X, AddressMode::ZeroPage),
            0x96 => (StoreRegister::X, AddressMode::ZeroPageY),
            0x8e => (StoreRegister::X, AddressMode::Absolute),
            0x84 => (StoreRegister::Y, AddressMode::ZeroPage),
            0x94 => (StoreRegister::Y, AddressMode::ZeroPageX),
            0x8c => (StoreRegister::Y, AddressMode::Absolute),
            _ => return None,
        };
        Some(instruction)
    }

    fn store_cycles(opcode: u8) -> u8 {
        match opcode {
            0x85 | 0x86 | 0x84 => 3,
            0x95 | 0x96 | 0x94 => 4,
            0x8d | 0x8e | 0x8c => 4,
            0x9d | 0x99 | 0x91 => 5 + u8::from(opcode == 0x91),
            0x81 => 6,
            _ => unreachable!("store opcode has a cycle count"),
        }
    }

    fn modify_cycles(opcode: u8) -> u8 {
        match opcode {
            0x0a | 0x4a | 0x2a | 0x6a => 2,
            0x06 | 0x46 | 0x26 | 0x66 | 0xe6 | 0xc6 => 5,
            0x16 | 0x56 | 0x36 | 0x76 | 0xf6 | 0xd6 => 6,
            0x0e | 0x4e | 0x2e | 0x6e | 0xee | 0xce => 6,
            _ => 7,
        }
    }

    fn read_operand(&mut self, bus: &Bus, mode: AddressMode) -> (u8, u8) {
        if let AddressMode::Immediate = mode {
            return (self.fetch_byte(bus), 2);
        }
        let (address, page_crossed) = self.address(bus, mode);
        let base_cycles = match mode {
            AddressMode::ZeroPage => 3,
            AddressMode::ZeroPageX | AddressMode::ZeroPageY => 4,
            AddressMode::Absolute => 4,
            AddressMode::AbsoluteX | AddressMode::AbsoluteY => 4,
            AddressMode::IndexedIndirect => 6,
            AddressMode::IndirectIndexed => 5,
            AddressMode::Immediate => unreachable!(),
        };
        let can_cross = matches!(
            mode,
            AddressMode::AbsoluteX | AddressMode::AbsoluteY | AddressMode::IndirectIndexed
        );
        (
            bus.read(address),
            base_cycles + u8::from(can_cross && page_crossed),
        )
    }

    fn address(&mut self, bus: &Bus, mode: AddressMode) -> (u16, bool) {
        match mode {
            AddressMode::Immediate => unreachable!(),
            AddressMode::ZeroPage => (u16::from(self.fetch_byte(bus)), false),
            AddressMode::ZeroPageX => (u16::from(self.fetch_byte(bus).wrapping_add(self.x)), false),
            AddressMode::ZeroPageY => (u16::from(self.fetch_byte(bus).wrapping_add(self.y)), false),
            AddressMode::Absolute => (self.fetch_word(bus), false),
            AddressMode::AbsoluteX => {
                let base = self.fetch_word(bus);
                let address = base.wrapping_add(u16::from(self.x));
                (address, base & 0xff00 != address & 0xff00)
            }
            AddressMode::AbsoluteY => {
                let base = self.fetch_word(bus);
                let address = base.wrapping_add(u16::from(self.y));
                (address, base & 0xff00 != address & 0xff00)
            }
            AddressMode::IndexedIndirect => {
                let pointer = self.fetch_byte(bus).wrapping_add(self.x);
                let low = bus.read(u16::from(pointer));
                let high = bus.read(u16::from(pointer.wrapping_add(1)));
                (u16::from_le_bytes([low, high]), false)
            }
            AddressMode::IndirectIndexed => {
                let pointer = self.fetch_byte(bus);
                let low = bus.read(u16::from(pointer));
                let high = bus.read(u16::from(pointer.wrapping_add(1)));
                let base = u16::from_le_bytes([low, high]);
                let address = base.wrapping_add(u16::from(self.y));
                (address, base & 0xff00 != address & 0xff00)
            }
        }
    }

    fn execute_read(&mut self, operation: ReadOperation, value: u8) {
        match operation {
            ReadOperation::Adc => self.adc(value),
            ReadOperation::And => {
                self.a &= value;
                self.set_zero_negative(self.a);
            }
            ReadOperation::Bit => {
                self.set_flag(ZERO, self.a & value == 0);
                self.set_flag(OVERFLOW, value & OVERFLOW != 0);
                self.set_flag(NEGATIVE, value & NEGATIVE != 0);
            }
            ReadOperation::CompareA => self.compare(self.a, value),
            ReadOperation::CompareX => self.compare(self.x, value),
            ReadOperation::CompareY => self.compare(self.y, value),
            ReadOperation::Eor => {
                self.a ^= value;
                self.set_zero_negative(self.a);
            }
            ReadOperation::LoadA => {
                self.a = value;
                self.set_zero_negative(self.a);
            }
            ReadOperation::LoadX => {
                self.x = value;
                self.set_zero_negative(self.x);
            }
            ReadOperation::LoadY => {
                self.y = value;
                self.set_zero_negative(self.y);
            }
            ReadOperation::Ora => {
                self.a |= value;
                self.set_zero_negative(self.a);
            }
            ReadOperation::Sbc => self.adc(!value),
        }
    }

    fn adc(&mut self, value: u8) {
        let carry = u16::from(self.flag(CARRY));
        let sum = u16::from(self.a) + u16::from(value) + carry;
        let result = sum as u8;
        self.set_flag(CARRY, sum > 0xff);
        self.set_flag(
            OVERFLOW,
            (!(self.a ^ value) & (self.a ^ result) & NEGATIVE) != 0,
        );
        self.a = result;
        self.set_zero_negative(self.a);
    }

    fn compare(&mut self, register: u8, value: u8) {
        self.set_flag(CARRY, register >= value);
        self.set_zero_negative(register.wrapping_sub(value));
    }

    fn modify(&mut self, operation: ModifyOperation, value: u8) -> u8 {
        let result = match operation {
            ModifyOperation::Asl => {
                self.set_flag(CARRY, value & NEGATIVE != 0);
                value.wrapping_shl(1)
            }
            ModifyOperation::Lsr => {
                self.set_flag(CARRY, value & CARRY != 0);
                value >> 1
            }
            ModifyOperation::Rol => {
                let carry = u8::from(self.flag(CARRY));
                self.set_flag(CARRY, value & NEGATIVE != 0);
                (value << 1) | carry
            }
            ModifyOperation::Ror => {
                let carry = u8::from(self.flag(CARRY)) << 7;
                self.set_flag(CARRY, value & CARRY != 0);
                (value >> 1) | carry
            }
            ModifyOperation::Inc => value.wrapping_add(1),
            ModifyOperation::Dec => value.wrapping_sub(1),
        };
        self.set_zero_negative(result);
        result
    }

    fn branch(&mut self, bus: &Bus, condition: bool) -> u8 {
        let offset = self.fetch_byte(bus) as i8;
        if !condition {
            return 2;
        }
        let old_pc = self.pc;
        self.pc = self.pc.wrapping_add_signed(i16::from(offset));
        3 + u8::from(old_pc & 0xff00 != self.pc & 0xff00)
    }

    fn fetch_byte(&mut self, bus: &Bus) -> u8 {
        let value = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    fn fetch_word(&mut self, bus: &Bus) -> u16 {
        let low = self.fetch_byte(bus);
        let high = self.fetch_byte(bus);
        u16::from_le_bytes([low, high])
    }

    fn read_word(bus: &Bus, address: u16) -> u16 {
        u16::from_le_bytes([bus.read(address), bus.read(address.wrapping_add(1))])
    }

    fn push(&mut self, bus: &mut Bus, value: u8) {
        bus.write(0x0100 | u16::from(self.sp), value);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pop(&mut self, bus: &mut Bus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 | u16::from(self.sp))
    }

    fn push_word(&mut self, bus: &mut Bus, value: u16) {
        let [low, high] = value.to_le_bytes();
        self.push(bus, high);
        self.push(bus, low);
    }

    fn pop_word(&mut self, bus: &mut Bus) -> u16 {
        let low = self.pop(bus);
        let high = self.pop(bus);
        u16::from_le_bytes([low, high])
    }

    fn flag(&self, flag: u8) -> bool {
        self.status & flag != 0
    }

    fn set_flag(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.status |= flag;
        } else {
            self.status &= !flag;
        }
        self.status |= UNUSED;
    }

    fn set_zero_negative(&mut self, value: u8) {
        self.set_flag(ZERO, value == 0);
        self.set_flag(NEGATIVE, value & NEGATIVE != 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu_with_program(program: &[u8]) -> (Cpu6502, Bus) {
        let mut bus = Bus::new();
        for (address, byte) in program.iter().copied().enumerate() {
            bus.write(address as u16, byte);
        }
        let mut cpu = Cpu6502::new();
        cpu.pc = 0;
        (cpu, bus)
    }

    #[test]
    fn load_and_adc_update_registers_and_flags() {
        let (mut cpu, mut bus) = cpu_with_program(&[0xa9, 0x7f, 0x69, 0x01]);

        assert_eq!(cpu.step(&mut bus), 2);
        assert_eq!(cpu.a, 0x7f);
        assert_eq!(cpu.step(&mut bus), 2);
        assert_eq!(cpu.a, 0x80);
        assert!(cpu.flag(OVERFLOW));
        assert!(cpu.flag(NEGATIVE));
        assert!(!cpu.flag(CARRY));
    }

    #[test]
    fn indexed_read_adds_cycle_when_crossing_page() {
        let (mut cpu, mut bus) = cpu_with_program(&[0xbd, 0xff, 0x00]);
        cpu.x = 1;
        bus.write(0x0100, 0x42);

        assert_eq!(cpu.step(&mut bus), 5);
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn indirect_jump_wraps_within_page() {
        let (mut cpu, mut bus) = cpu_with_program(&[]);
        cpu.pc = 0x0200;
        bus.write(0x0200, 0x6c);
        bus.write(0x0201, 0xff);
        bus.write(0x0202, 0x00);
        bus.write(0x00ff, 0x34);
        bus.write(0x0000, 0x12);
        bus.write(0x0100, 0x99);

        assert_eq!(cpu.step(&mut bus), 5);
        assert_eq!(cpu.pc, 0x1234);
    }

    #[test]
    fn branch_adds_cycles_for_taken_page_crossing() {
        let (mut cpu, mut bus) = cpu_with_program(&[]);
        cpu.pc = 0x00fd;
        bus.write(0x00fd, 0xd0);
        bus.write(0x00fe, 0x01);

        assert_eq!(cpu.step(&mut bus), 4);
        assert_eq!(cpu.pc, 0x0100);
    }

    #[test]
    fn store_writes_register_to_zero_page() {
        let (mut cpu, mut bus) = cpu_with_program(&[0x85, 0x42]);
        cpu.a = 0xa5;

        assert_eq!(cpu.step(&mut bus), 3);
        assert_eq!(bus.read(0x42), 0xa5);
    }
}

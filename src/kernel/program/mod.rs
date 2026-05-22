#![allow(non_camel_case_types)]

use core::fmt::Display;

pub enum LoaderError {
    InvalidBuffer,
    InvalidMagicNumbers,
    NotAWashingMachine,
    Not64BitElf,
    UnsupportedElfType(u16),
    UnsupportedArch(u16),
    UnsupportedABI(u8),
}

impl Display for LoaderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoaderError::InvalidBuffer => write!(f, "InvalidBuffer"),
            LoaderError::InvalidMagicNumbers => write!(f, "Invalid ELF Magic numbers"),
            LoaderError::NotAWashingMachine => write!(f, "Big endian not supported"),
            LoaderError::Not64BitElf => write!(f, "32 bit programs not supported"),
            LoaderError::UnsupportedElfType(t) => write!(f, "Unsupported ELF type: 0x{:X}", t),
            LoaderError::UnsupportedArch(t) => write!(f, "Unsupported architechture: 0x{:X}", t),
            LoaderError::UnsupportedABI(t) => write!(f, "Unsupported ABI: 0x{:X}", t),
        }
    }
}

pub type Elf64_Addr = u64;              // u prog addr
pub type Elf64_Off = u64;               // u file offset
pub type Elf64_Half = u16;              // u medium int
pub type Elf64_Word = u32;              // u int
pub type Elf64_Sword = i32;             // s int 
pub type Elf64_Xword = u64;             // u long int
pub type Elf64_Sxword = i64;            // s long int

pub const EI_NIDENT: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64_Ehdr {
    pub e_ident: [u8; EI_NIDENT],       // elf ident
    pub e_type: Elf64_Half,             // obj file type
    pub e_machine: Elf64_Half,          // machine type
    pub e_version: Elf64_Word,          // obj file version
    pub e_entry: Elf64_Addr,            // entry point addr 
    pub e_phoff: Elf64_Off,             // prog header offset
    pub e_shoff: Elf64_Off,             // section header offset
    pub e_flags: Elf64_Word,            // cpu specific flags
    pub e_ehsize: Elf64_Half,           // elf header size
    pub e_phentsize: Elf64_Half,        // size of prog header entry 
    pub e_phnum: Elf64_Half,            // number of prog header entreis
    pub e_shentsize: Elf64_Half,        // size of section header entry
    pub e_shnum: Elf64_Half,            // number of section header entries
    pub e_shstrndx: Elf64_Half,         // section name string table idx
}

pub const ELF_MAGIC_NUMBERS: [u8; 4] = [0x7F, b'E', b'L', b'F'];

#[repr(u16)]
pub enum E_Type {
    ET_NONE = 0,                        // no file type
    ET_REL = 1,                         // relocatable obj file
    ET_EXEC = 2,                        // executable file
    ET_DYN = 3,                         // shared obj file
    ET_CORE = 4,                        // core file
    ET_LOOS = 0xFE00,                   // env specific use
    ET_HIOS = 0xFEFF,                   // 
    ET_LOPROC = 0xFF00,                 // cpu specific use
    ET_HIPROC = 0xFFFF,                 // 
}

// ignore big endian because why would i ever use that i'm not running this on a washing machine
impl Elf64_Ehdr {
    pub fn from_bytes(bytes: &[u8]) -> Result<&Self, LoaderError> {
        if bytes.len() < size_of::<Self>() { return Err(LoaderError::InvalidBuffer); }
        if bytes[0..4] != ELF_MAGIC_NUMBERS { return Err(LoaderError::InvalidMagicNumbers); }

        unsafe {
            Ok(&*(bytes.as_ptr() as *const Self))
        }
    }

    pub fn get_type(&self) -> Result<E_Type, LoaderError> {
        match self.e_type {
            0 => Ok(E_Type::ET_NONE),
            1 => Ok(E_Type::ET_REL),
            2 => Ok(E_Type::ET_EXEC),
            3 => Ok(E_Type::ET_DYN),
            4 => Ok(E_Type::ET_CORE),
            0xFE00 => Ok(E_Type::ET_LOOS),
            0xFEFF => Ok(E_Type::ET_HIOS),
            0xFF00 => Ok(E_Type::ET_LOPROC),
            0xFFFF => Ok(E_Type::ET_HIPROC),
            _ => Err(LoaderError::UnsupportedElfType(self.e_type)),
        }
    }

    pub fn validate(&self) -> Result<(), LoaderError> {
        match self.e_ident[4] {
            2 => {},
            _ => return Err(LoaderError::Not64BitElf),
        }

        match self.e_ident[5] {
            1 => {},
            _ => return Err(LoaderError::NotAWashingMachine),
        }

        match self.e_ident[7] {
            0 => {},
            _ => return Err(LoaderError::UnsupportedABI(self.e_ident[7])),
        }

        // explicitly matching arm and risc-v because i might support them later
        match self.e_machine {
            0x3E => {},
            0xB7 => return Err(LoaderError::UnsupportedArch(self.e_machine)),
            0xF3 => return Err(LoaderError::UnsupportedArch(self.e_machine)),
            _ => return Err(LoaderError::UnsupportedArch(self.e_machine)),
        }

        Ok(())
    }
}

//! Binary format parsing — ELF, Mach-O, PE via goblin.

use anyhow::{Context, Result};
use goblin::Object;
use serde::Serialize;
use std::path::Path;

/// Basic binary metadata.
#[derive(Debug, Serialize)]
pub struct BinaryInfo {
    /// File name.
    pub name: String,
    /// File size in bytes.
    pub size: u64,
    /// Binary format: "ELF", "Mach-O", "PE", "Archive", "Unknown".
    pub format: String,
    /// Architecture: "x86_64", "aarch64", "i386", etc.
    pub arch: String,
    /// Whether the binary is a shared library.
    pub is_lib: bool,
    /// Whether the binary is stripped (no debug symbols).
    pub is_stripped: bool,
    /// Entry point address (0 for libraries).
    pub entry_point: u64,
    /// Number of sections/segments.
    pub sections: usize,
}

/// An extracted symbol (function, variable, type).
#[derive(Debug, Clone, Serialize)]
pub struct ExportedSymbol {
    /// Symbol name (may be mangled).
    pub name: String,
    /// Demangled name if available.
    pub demangled: Option<String>,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Virtual address.
    pub address: u64,
    /// Size in bytes (0 if unknown).
    pub size: u64,
    /// Whether this is an import (from another library).
    pub is_import: bool,
}

/// Symbol classification.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Data,
    Section,
    File,
    Unknown,
}

/// Parse a binary and extract basic info.
pub fn parse_binary(data: &[u8], path: &Path) -> Result<BinaryInfo> {
    let obj = Object::parse(data).context("Failed to parse binary")?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = data.len() as u64;

    match obj {
        Object::Elf(elf) => {
            let arch = match elf.header.e_machine {
                goblin::elf::header::EM_X86_64 => "x86_64",
                goblin::elf::header::EM_AARCH64 => "aarch64",
                goblin::elf::header::EM_386 => "i386",
                goblin::elf::header::EM_ARM => "arm",
                goblin::elf::header::EM_RISCV => "riscv",
                _ => "unknown",
            };
            let is_lib = elf.header.e_type == goblin::elf::header::ET_DYN;
            let is_stripped = elf.syms.is_empty();
            Ok(BinaryInfo {
                name,
                size,
                format: "ELF".into(),
                arch: arch.into(),
                is_lib,
                is_stripped,
                entry_point: elf.header.e_entry,
                sections: elf.section_headers.len(),
            })
        }
        Object::Mach(mach) => match mach {
            goblin::mach::Mach::Binary(macho) => {
                let arch = match macho.header.cputype() {
                    goblin::mach::cputype::CPU_TYPE_X86_64 => "x86_64",
                    goblin::mach::cputype::CPU_TYPE_ARM64 => "aarch64",
                    goblin::mach::cputype::CPU_TYPE_X86 => "i386",
                    goblin::mach::cputype::CPU_TYPE_ARM => "arm",
                    _ => "unknown",
                };
                let is_lib = macho.header.filetype == goblin::mach::header::MH_DYLIB;
                let is_stripped = macho.symbols().count() == 0;
                Ok(BinaryInfo {
                    name,
                    size,
                    format: "Mach-O".into(),
                    arch: arch.into(),
                    is_lib,
                    is_stripped,
                    entry_point: macho.entry,
                    sections: macho.segments.len(),
                })
            }
            goblin::mach::Mach::Fat(fat) => Ok(BinaryInfo {
                name,
                size,
                format: "Mach-O (Universal)".into(),
                arch: format!("{} architectures", fat.narches),
                is_lib: false,
                is_stripped: false,
                entry_point: 0,
                sections: 0,
            }),
        },
        Object::PE(pe) => {
            let arch = if pe.is_64 { "x86_64" } else { "i386" };
            let is_lib = pe.is_lib;
            let is_stripped = pe.exports.is_empty() && pe.imports.is_empty();
            Ok(BinaryInfo {
                name,
                size,
                format: "PE".into(),
                arch: arch.into(),
                is_lib,
                is_stripped,
                entry_point: pe.entry as u64,
                sections: pe.sections.len(),
            })
        }
        Object::Archive(_) => Ok(BinaryInfo {
            name,
            size,
            format: "Archive".into(),
            arch: "multiple".into(),
            is_lib: true,
            is_stripped: false,
            entry_point: 0,
            sections: 0,
        }),
        _ => Ok(BinaryInfo {
            name,
            size,
            format: "Unknown".into(),
            arch: "unknown".into(),
            is_lib: false,
            is_stripped: true,
            entry_point: 0,
            sections: 0,
        }),
    }
}

/// Extract symbols from a binary.
pub fn extract_symbols(data: &[u8]) -> Result<Vec<ExportedSymbol>> {
    let obj = Object::parse(data).context("Failed to parse binary")?;
    let mut symbols = Vec::new();

    match obj {
        Object::Elf(elf) => {
            // Dynamic symbols (exports/imports of shared libraries)
            for sym in elf.dynsyms.iter() {
                if let Some(name) = elf.dynstrtab.get_at(sym.st_name) {
                    if name.is_empty() {
                        continue;
                    }
                    symbols.push(ExportedSymbol {
                        demangled: demangle_name(name),
                        name: name.to_string(),
                        kind: elf_sym_kind(sym.st_type()),
                        address: sym.st_value,
                        size: sym.st_size,
                        is_import: sym.is_import(),
                    });
                }
            }
            // Regular symbol table (if not stripped)
            for sym in elf.syms.iter() {
                if let Some(name) = elf.strtab.get_at(sym.st_name) {
                    if name.is_empty() || name.starts_with('$') {
                        continue;
                    }
                    // Skip if already in dynsyms
                    if symbols.iter().any(|s| s.name == name && s.address == sym.st_value) {
                        continue;
                    }
                    symbols.push(ExportedSymbol {
                        demangled: demangle_name(name),
                        name: name.to_string(),
                        kind: elf_sym_kind(sym.st_type()),
                        address: sym.st_value,
                        size: sym.st_size,
                        is_import: sym.is_import(),
                    });
                }
            }
        }
        Object::Mach(mach) => {
            if let goblin::mach::Mach::Binary(macho) = mach {
                for sym_result in macho.symbols() {
                    let (name, nlist) = match sym_result {
                        Ok(pair) => pair,
                        Err(_) => continue,
                    };
                    if name.is_empty() {
                        continue;
                    }
                    // Strip leading underscore (Mach-O convention)
                    let clean_name = name.strip_prefix('_').unwrap_or(name);
                    let is_import = nlist.is_undefined();
                    let kind = if nlist.get_type() == goblin::mach::symbols::N_SECT {
                        // Use heuristic: functions are usually in __TEXT,__text
                        SymbolKind::Function
                    } else {
                        SymbolKind::Unknown
                    };
                    symbols.push(ExportedSymbol {
                        demangled: demangle_name(clean_name),
                        name: clean_name.to_string(),
                        kind,
                        address: nlist.n_value,
                        size: 0,
                        is_import,
                    });
                }
            }
        }
        Object::PE(pe) => {
            for export in &pe.exports {
                if let Some(name) = export.name {
                    symbols.push(ExportedSymbol {
                        demangled: demangle_name(name),
                        name: name.to_string(),
                        kind: SymbolKind::Function,
                        address: export.rva as u64,
                        size: export.size as u64,
                        is_import: false,
                    });
                }
            }
            for import in &pe.imports {
                symbols.push(ExportedSymbol {
                    demangled: None,
                    name: import.name.to_string(),
                    kind: SymbolKind::Function,
                    address: import.rva as u64,
                    size: 0,
                    is_import: true,
                });
            }
        }
        _ => {}
    }

    // Sort by address for deterministic output
    symbols.sort_by_key(|s| s.address);
    Ok(symbols)
}

/// Convert ELF symbol type to our kind.
fn elf_sym_kind(st_type: u8) -> SymbolKind {
    match st_type {
        goblin::elf::sym::STT_FUNC => SymbolKind::Function,
        goblin::elf::sym::STT_OBJECT => SymbolKind::Data,
        goblin::elf::sym::STT_SECTION => SymbolKind::Section,
        goblin::elf::sym::STT_FILE => SymbolKind::File,
        _ => SymbolKind::Unknown,
    }
}

/// Try to demangle a Rust or C++ symbol name.
fn demangle_name(name: &str) -> Option<String> {
    // Rust v0 mangling: starts with _R
    // Rust legacy: starts with _ZN
    // C++ mangling: starts with _Z
    if name.starts_with("_R") || name.starts_with("_ZN") || name.starts_with("_Z") {
        // Use rustc-demangle-style heuristic
        let demangled = rustc_demangle(name);
        if demangled != name {
            return Some(demangled);
        }
    }
    None
}

/// Simple Rust symbol demangling (handles _ZN...E pattern).
/// For production use, consider the `rustc-demangle` crate.
fn rustc_demangle(mangled: &str) -> String {
    // Skip leading underscore
    let s = mangled.strip_prefix('_').unwrap_or(mangled);

    // Rust legacy mangling: ZN <length><name>... E
    if let Some(rest) = s.strip_prefix("ZN") {
        if let Some(demangled) = parse_rust_legacy(rest) {
            return demangled;
        }
    }

    mangled.to_string()
}

/// Parse Rust legacy mangling (_ZN<len><name><len><name>...E).
fn parse_rust_legacy(s: &str) -> Option<String> {
    let mut parts = Vec::new();
    let mut pos = 0;
    let chars: Vec<char> = s.chars().collect();

    while pos < chars.len() {
        if chars[pos] == 'E' {
            break;
        }
        // Read length
        let start = pos;
        while pos < chars.len() && chars[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == start {
            return None; // No length found
        }
        let len: usize = chars[start..pos].iter().collect::<String>().parse().ok()?;
        if pos + len > chars.len() {
            return None;
        }
        let part: String = chars[pos..pos + len].iter().collect();
        // Filter hash suffix (17-char hex like h followed by hex digits)
        if !(part.starts_with('h') && part.len() == 17 && part[1..].chars().all(|c| c.is_ascii_hexdigit())) {
            parts.push(part);
        }
        pos += len;
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("::"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_legacy() {
        // _ZN3foo3bar17h1234567890abcdefE
        let result = parse_rust_legacy("3foo3bar17h1234567890abcdefE");
        assert_eq!(result, Some("foo::bar".to_string()));
    }

    #[test]
    fn test_parse_rust_legacy_simple() {
        let result = parse_rust_legacy("5hello5worldE");
        assert_eq!(result, Some("hello::world".to_string()));
    }

    #[test]
    fn test_demangle_name_non_mangled() {
        assert_eq!(demangle_name("printf"), None);
    }
}

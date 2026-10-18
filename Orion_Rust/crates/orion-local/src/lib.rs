//! REGRAS.md: modelo local é reserva sem ferramentas; opt-in não liga sozinho.
pub mod catalog;
pub mod download;
#[cfg(windows)]
pub mod job;
pub mod protocol;
pub mod wake;

pub const MAX_RAM_PERCENT: u64 = 30;
pub const RUNTIME_RESERVE: u64 = 768 * 1024 * 1024;

/// Margem conservadora para pesos, contexto curto e buffers. O Job Object impõe o teto real.
pub fn admitted(model_bytes: u64, total_ram: u64, available_ram: u64) -> bool {
    let limit = ram_limit(total_ram);
    let estimate = model_bytes.saturating_add(RUNTIME_RESERVE);
    limit > 0 && estimate <= limit && estimate <= available_ram.saturating_sub(256 * 1024 * 1024)
}

pub fn ram_limit(total_ram: u64) -> u64 {
    total_ram / 100 * MAX_RAM_PERCENT
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn teto_e_disponibilidade_sao_independentes() {
        let gb = 1024 * 1024 * 1024;
        assert!(admitted(gb, 8 * gb, 4 * gb));
        assert!(!admitted(3 * gb, 8 * gb, 7 * gb));
        assert!(!admitted(gb, 32 * gb, gb));
        assert!(!admitted(u64::MAX, 32 * gb, 20 * gb));
        assert!(ram_limit(32 * gb) <= 32 * gb * 30 / 100);
    }
}

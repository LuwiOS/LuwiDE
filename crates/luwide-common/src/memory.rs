use serde::{Deserialize, Serialize};

/// Orçamento de memória por módulo. Meta total do LuwiDE: ~200 MB,
/// para rodar bem em hardware modesto (e, no futuro, no LuwiOS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryBudget {
    pub compositor_mb: u32,
    pub shell_mb: u32,
    pub panel_mb: u32,
    pub launcher_mb: u32,
    pub sessiond_mb: u32,
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self {
            compositor_mb: 70,
            shell_mb: 60,
            panel_mb: 20,
            launcher_mb: 25,
            sessiond_mb: 25,
        }
    }
}

impl MemoryBudget {
    pub fn total_mb(&self) -> u32 {
        self.compositor_mb + self.shell_mb + self.panel_mb + self.launcher_mb + self.sessiond_mb
    }

    /// Orçamento de um módulo pelo nome ("shell", "panel", ...).
    pub fn for_module(&self, module: &str) -> Option<u32> {
        Some(match module {
            "compositor" => self.compositor_mb,
            "shell" => self.shell_mb,
            "panel" => self.panel_mb,
            "launcher" => self.launcher_mb,
            "sessiond" => self.sessiond_mb,
            _ => return None,
        })
    }
}

/// RSS atual do processo em MB, lido de `/proc/self/status` (Linux).
/// Retorna `None` fora do Linux ou se a leitura falhar.
pub fn current_rss_mb() -> Option<u64> {
    let raw = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_total_e_200mb() {
        assert_eq!(MemoryBudget::default().total_mb(), 200);
        assert_eq!(MemoryBudget::default().for_module("shell"), Some(60));
        assert_eq!(MemoryBudget::default().for_module("nada"), None);
    }
}

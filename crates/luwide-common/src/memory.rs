use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

use anyhow::Result;
use luwide_common::{ConfigStore, SessionSnapshot};
use tracing::{info, warn};

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let mut config = ConfigStore::load_or_create()?;
    let snapshot = SessionSnapshot::new(1, vec!["Explorer".into(), "Terminal".into()]);
    info!(workspace = snapshot.focused_workspace, clock = %snapshot.clock, "session daemon online");

    loop {
        if config.reload_if_changed()? {
            info!(version = config.current().version, "configuração recarregada a quente");
        }

        let budget = &config.current().memory_budget;
        let total = budget.total_mb();
        info!(compositor=budget.compositor_mb, shell=budget.shell_mb, panel=budget.panel_mb, launcher=budget.launcher_mb, sessiond=budget.sessiond_mb, total, "orçamento de memória por módulo");

        let current_rss = current_rss_mb().unwrap_or_default();
        if current_rss > budget.sessiond_mb as u64 {
            warn!(rss_mb=current_rss, budget_mb=budget.sessiond_mb, "sessiond acima do orçamento");
        } else {
            info!(rss_mb=current_rss, budget_mb=budget.sessiond_mb, "sessiond dentro do orçamento");
        }

        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

fn current_rss_mb() -> Option<u64> {
    let raw = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

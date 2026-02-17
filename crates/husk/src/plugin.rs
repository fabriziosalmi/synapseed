use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::{Severity, SynapseEvent};
use synapseed_core::liquid::ProjectDna;
use synapseed_core::plugin::SynapsePlugin;
use synapseed_core::policy::SecurityPolicy;
use tracing::{info, warn};

use crate::guard::SecurityGuard;

/// The Husk plugin — DLP security enforcement.
pub struct HuskPlugin {
    guard: Arc<SecurityGuard>,
}

impl HuskPlugin {
    pub fn new() -> Self {
        Self {
            guard: Arc::new(SecurityGuard::with_defaults()),
        }
    }

    /// Create a HuskPlugin configured from project DNA.
    /// Custom DLP rules from the DNA are forwarded to the SecurityGuard.
    pub fn from_dna(dna: &ProjectDna) -> Self {
        let policy = SecurityPolicy {
            dlp_rules: dna.dlp_custom_rules.clone(),
            command_rules: Vec::new(),
            fail_closed: true,
            dlp_whitelist: dna.dlp_whitelist.clone(),
        };
        Self {
            guard: Arc::new(SecurityGuard::from_policy(&policy)),
        }
    }

    pub fn guard(&self) -> &SecurityGuard {
        &self.guard
    }
}

impl Default for HuskPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for HuskPlugin {
    fn name(&self) -> &str {
        "husk"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        ctx.set_extension(self.guard.clone());
        let dlp_level = ctx.dna().dlp_level;
        info!(level = ?dlp_level, "Husk: Security shield active");
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move {
            match event {
                // Scan file content on change for accidental secret commits
                SynapseEvent::FileChanged { path, .. } => {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        match self.guard.check(&content) {
                            Ok(()) => {}
                            Err(e) => {
                                warn!(file = %path, error = %e, "Husk: Secrets detected in file");
                                ctx.update_metrics(|m| m.dlp_blocks += 1);
                                return Ok(Some(SynapseEvent::SecurityAlert {
                                    rule: "file_scan".into(),
                                    severity: Severity::High,
                                    context: format!("Secrets found in {path}"),
                                }));
                            }
                        }
                    }
                    ctx.update_metrics(|m| m.dlp_scans += 1);
                    Ok(None)
                }
                _ => Ok(None),
            }
        })
    }

    fn priority(&self) -> u32 {
        10 // Highest priority — security checks run first
    }
}

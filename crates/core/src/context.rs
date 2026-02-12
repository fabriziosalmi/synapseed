use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;
use tracing::debug;

use crate::event::SynapseEvent;
use crate::liquid::ProjectDna;
use crate::state::ProjectState;

/// Thread-safe shared state + async Event Bus for all plugins.
///
/// The context is both a data store (project state, metrics, config)
/// and a nervous system (broadcast channel for domain events).
///
/// Cloning is cheap (Arc internally).
#[derive(Clone)]
pub struct SynapseContext {
    inner: Arc<RwLock<ContextInner>>,
    /// Async broadcast sender — plugins subscribe via `subscribe()`
    event_tx: broadcast::Sender<SynapseEvent>,
    /// Type-erased extensions — plugins register shared objects for cross-crate access.
    extensions: Arc<RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
}

struct ContextInner {
    project_root: PathBuf,
    project_state: ProjectState,
    dna: ProjectDna,
    metrics: ContextMetrics,
}

/// Runtime metrics tracked across the session.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ContextMetrics {
    pub files_indexed: usize,
    pub symbols_found: usize,
    pub dlp_scans: usize,
    pub dlp_blocks: usize,
    pub commands_allowed: usize,
    pub commands_denied: usize,
    pub errors_prevented: usize,
    pub events_broadcast: usize,
}

impl SynapseContext {
    /// Create a new context with an event bus (capacity = 256 events).
    pub fn new(project_root: PathBuf, state: ProjectState, dna: ProjectDna) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        Self {
            inner: Arc::new(RwLock::new(ContextInner {
                project_root,
                project_state: state,
                dna,
                metrics: ContextMetrics::default(),
            })),
            event_tx,
            extensions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ── Data Accessors ──────────────────────────────────────────

    pub fn project_root(&self) -> PathBuf {
        self.inner.read().unwrap().project_root.clone()
    }

    pub fn project_state(&self) -> ProjectState {
        self.inner.read().unwrap().project_state.clone()
    }

    pub fn dna(&self) -> ProjectDna {
        self.inner.read().unwrap().dna.clone()
    }

    pub fn metrics(&self) -> ContextMetrics {
        self.inner.read().unwrap().metrics.clone()
    }

    pub fn update_metrics<F: FnOnce(&mut ContextMetrics)>(&self, f: F) {
        let mut inner = self.inner.write().unwrap();
        f(&mut inner.metrics);
    }

    pub fn set_project_state(&self, state: ProjectState) {
        self.inner.write().unwrap().project_state = state;
    }

    // ── Event Bus ───────────────────────────────────────────────

    /// Broadcast an event to all subscribers.
    /// Returns the number of receivers that got the event.
    pub fn broadcast(&self, event: SynapseEvent) -> usize {
        debug!(event_type = ?std::mem::discriminant(&event), "Broadcasting event");
        self.update_metrics(|m| m.events_broadcast += 1);
        self.event_tx.send(event).unwrap_or(0)
    }

    /// Subscribe to the event bus. Returns a receiver for async consumption.
    pub fn subscribe(&self) -> broadcast::Receiver<SynapseEvent> {
        self.event_tx.subscribe()
    }

    /// Get a clone of the sender (for spawning background tasks that emit events).
    pub fn event_sender(&self) -> broadcast::Sender<SynapseEvent> {
        self.event_tx.clone()
    }

    // ── Extensions ──────────────────────────────────────────────

    /// Register a shared object by type. Plugins set these during on_init.
    pub fn set_extension<T: Send + Sync + 'static>(&self, ext: Arc<T>) {
        let mut map = self.extensions.write().unwrap();
        map.insert(TypeId::of::<T>(), ext);
    }

    /// Retrieve a shared object by type. MCP tools and other plugins read these.
    pub fn get_extension<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let map = self.extensions.read().unwrap();
        map.get(&TypeId::of::<T>())
            .and_then(|ext| ext.clone().downcast::<T>().ok())
    }
}

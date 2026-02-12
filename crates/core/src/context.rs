use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::event::SynapseEvent;
use crate::liquid::ProjectDna;
use crate::state::ProjectState;

// ══════════════════════════════════════════════════════════════
// EXTRACTED COMPONENT 1: EventBus
// Decoupled pub/sub — plugins that only need events can accept
// `&EventBus` instead of the full SynapseContext.
// ══════════════════════════════════════════════════════════════

/// Typed broadcast event bus, decoupled from the context.
///
/// Plugins that only need pub/sub can accept `&EventBus` instead
/// of the full [`SynapseContext`], reducing coupling.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<SynapseEvent>,
}

impl EventBus {
    /// Create a new event bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Broadcast an event to all subscribers.
    /// Returns the number of receivers that got the event.
    pub fn broadcast(&self, event: SynapseEvent) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    /// Subscribe to the event bus. Returns a receiver for async consumption.
    pub fn subscribe(&self) -> broadcast::Receiver<SynapseEvent> {
        self.tx.subscribe()
    }

    /// Get a clone of the sender (for spawning background tasks that emit events).
    pub fn sender(&self) -> broadcast::Sender<SynapseEvent> {
        self.tx.clone()
    }
}

// ══════════════════════════════════════════════════════════════
// EXTRACTED COMPONENT 2: ShutdownCoordinator
// Lifecycle management decoupled from data/events.
// ══════════════════════════════════════════════════════════════

/// Coordinated shutdown for both async and sync contexts.
///
/// Wraps a [`CancellationToken`] for async tasks and an [`AtomicBool`]
/// flag for `std::thread` loops that cannot await.
#[derive(Clone)]
pub struct ShutdownCoordinator {
    token: CancellationToken,
    flag: Arc<AtomicBool>,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns a clone of the cancellation token.
    /// Background async tasks should use `token.cancelled().await` in a
    /// `tokio::select!` branch to exit their loops.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Returns a clone of the shutdown flag for std::thread loops.
    /// Check with `flag.load(Ordering::Relaxed)` in loop conditions.
    pub fn flag(&self) -> Arc<AtomicBool> {
        self.flag.clone()
    }

    /// Trigger shutdown: sets the flag and cancels the token.
    pub fn request_shutdown(&self) {
        self.flag.store(true, Ordering::Relaxed);
        self.token.cancel();
    }

    /// Returns true if shutdown has been requested.
    pub fn is_shutting_down(&self) -> bool {
        self.token.is_cancelled()
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════
// EXTRACTED COMPONENT 3: ExtensionRegistry
// Type-erased DI container, standalone and testable.
// ══════════════════════════════════════════════════════════════

/// Type-erased dependency injection container.
///
/// Plugins register shared objects during `on_init`; MCP tools
/// and other plugins retrieve them by type.
#[derive(Clone)]
pub struct ExtensionRegistry {
    map: Arc<RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self {
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a shared object by type.
    pub fn set<T: Send + Sync + 'static>(&self, ext: Arc<T>) {
        self.map.write().insert(TypeId::of::<T>(), ext);
    }

    /// Retrieve a shared object by type.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let guard = self.map.read();
        guard
            .get(&TypeId::of::<T>())
            .and_then(|ext| ext.clone().downcast::<T>().ok())
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════
// CONTEXT METRICS
// ══════════════════════════════════════════════════════════════

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
    pub tools_invoked: usize,
}

// ══════════════════════════════════════════════════════════════
// SYNAPSE CONTEXT — Composed Facade (identical public API)
// ══════════════════════════════════════════════════════════════

struct ContextInner {
    project_root: PathBuf,
    project_state: ProjectState,
    dna: ProjectDna,
    metrics: ContextMetrics,
}

/// Thread-safe shared state + async Event Bus for all plugins.
///
/// Internally composed of decoupled components:
/// - [`EventBus`] for pub/sub
/// - [`ShutdownCoordinator`] for lifecycle
/// - [`ExtensionRegistry`] for type-erased DI
///
/// Cloning is cheap (Arc internally).
#[derive(Clone)]
pub struct SynapseContext {
    inner: Arc<RwLock<ContextInner>>,
    event_bus: Arc<EventBus>,
    extensions: ExtensionRegistry,
    shutdown: Arc<ShutdownCoordinator>,
}

impl SynapseContext {
    /// Create a new context with an event bus (capacity = 4096 events).
    ///
    /// A large capacity prevents `Lagged` errors when plugins produce
    /// events faster than consumers drain them (e.g., bulk file indexing).
    pub fn new(project_root: PathBuf, state: ProjectState, dna: ProjectDna) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ContextInner {
                project_root,
                project_state: state,
                dna,
                metrics: ContextMetrics::default(),
            })),
            event_bus: Arc::new(EventBus::new(4096)),
            extensions: ExtensionRegistry::new(),
            shutdown: Arc::new(ShutdownCoordinator::new()),
        }
    }

    // ── Data Accessors ──────────────────────────────────────────

    pub fn project_root(&self) -> PathBuf {
        self.inner.read().project_root.clone()
    }

    pub fn project_state(&self) -> ProjectState {
        self.inner.read().project_state.clone()
    }

    pub fn dna(&self) -> ProjectDna {
        self.inner.read().dna.clone()
    }

    pub fn metrics(&self) -> ContextMetrics {
        self.inner.read().metrics.clone()
    }

    pub fn update_metrics<F: FnOnce(&mut ContextMetrics)>(&self, f: F) {
        let mut inner = self.inner.write();
        f(&mut inner.metrics);
    }

    pub fn set_project_state(&self, state: ProjectState) {
        self.inner.write().project_state = state;
    }

    // ── Event Bus (delegates to EventBus) ────────────────────────

    /// Broadcast an event to all subscribers.
    /// Returns the number of receivers that got the event.
    pub fn broadcast(&self, event: SynapseEvent) -> usize {
        debug!(event_type = ?std::mem::discriminant(&event), "Broadcasting event");
        self.update_metrics(|m| m.events_broadcast += 1);
        self.event_bus.broadcast(event)
    }

    /// Subscribe to the event bus. Returns a receiver for async consumption.
    pub fn subscribe(&self) -> broadcast::Receiver<SynapseEvent> {
        self.event_bus.subscribe()
    }

    /// Get a clone of the sender (for spawning background tasks that emit events).
    pub fn event_sender(&self) -> broadcast::Sender<SynapseEvent> {
        self.event_bus.sender()
    }

    // ── Extensions (delegates to ExtensionRegistry) ──────────────

    /// Register a shared object by type. Plugins set these during on_init.
    pub fn set_extension<T: Send + Sync + 'static>(&self, ext: Arc<T>) {
        self.extensions.set(ext);
    }

    /// Retrieve a shared object by type. MCP tools and other plugins read these.
    pub fn get_extension<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.extensions.get()
    }

    // ── Shutdown (delegates to ShutdownCoordinator) ──────────────

    /// Returns a clone of the cancellation token.
    /// Background async tasks should use `token.cancelled().await` in a
    /// `tokio::select!` branch to exit their loops.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.token()
    }

    /// Returns a clone of the shutdown flag for std::thread loops.
    /// Check with `flag.load(Ordering::Relaxed)` in loop conditions.
    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        self.shutdown.flag()
    }

    /// Trigger shutdown: sets the flag and cancels the token.
    /// Called once from the signal handler in main.rs.
    pub fn request_shutdown(&self) {
        self.shutdown.request_shutdown();
    }

    /// Returns true if shutdown has been requested.
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown.is_shutting_down()
    }
}

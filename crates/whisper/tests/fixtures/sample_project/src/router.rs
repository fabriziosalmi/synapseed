/// HTTP router — maps request paths to handler functions.

/// HTTP method enum.
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

/// A route definition mapping a path pattern to a handler.
pub struct Route {
    pub method: Method,
    pub path: String,
    pub handler: fn() -> String,
}

/// The main router that holds all registered routes.
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    /// Create a new empty router.
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Register a GET route.
    pub fn get(&mut self, path: &str, handler: fn() -> String) {
        self.routes.push(Route {
            method: Method::Get,
            path: path.to_string(),
            handler,
        });
    }

    /// Register a POST route.
    pub fn post(&mut self, path: &str, handler: fn() -> String) {
        self.routes.push(Route {
            method: Method::Post,
            path: path.to_string(),
            handler,
        });
    }

    /// Dispatch a request to the matching handler.
    pub fn dispatch(&self, method: &Method, path: &str) -> Option<String> {
        for route in &self.routes {
            if route.path == path && matches!((&route.method, method),
                (Method::Get, Method::Get) |
                (Method::Post, Method::Post) |
                (Method::Put, Method::Put) |
                (Method::Delete, Method::Delete)
            ) {
                return Some((route.handler)());
            }
        }
        None
    }

    /// Get the number of registered routes.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

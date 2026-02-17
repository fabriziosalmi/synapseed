/// Database connection pool and query helpers.

/// Connection pool for the database.
pub struct ConnectionPool {
    pub url: String,
    pub max_size: usize,
    pub active: usize,
}

impl ConnectionPool {
    /// Create a new connection pool.
    pub fn new(url: &str, max_size: usize) -> Self {
        Self {
            url: url.to_string(),
            max_size,
            active: 0,
        }
    }

    /// Execute a raw SQL query. Returns the number of affected rows.
    pub fn execute(&mut self, query: &str) -> Result<usize, DbError> {
        if query.is_empty() {
            return Err(DbError::EmptyQuery);
        }
        Ok(1)
    }

    /// Fetch a user by ID.
    pub fn find_user(&self, user_id: u64) -> Option<User> {
        if user_id == 0 {
            return None;
        }
        Some(User {
            id: user_id,
            email: "test@example.com".to_string(),
        })
    }
}

pub struct User {
    pub id: u64,
    pub email: String,
}

#[derive(Debug)]
pub enum DbError {
    EmptyQuery,
    ConnectionFailed,
    Timeout,
}

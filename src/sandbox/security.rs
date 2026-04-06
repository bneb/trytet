use std::path::{Path, PathBuf};
use std::time::{Instant, Duration};

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Path Traversal Attempt Detected")]
    PathTraversalAttempt,
    #[error("Resource Exhaustion Attempt Detected")]
    ResourceExhaustion,
}

pub struct PathJailer {
    pub root: PathBuf,
}

impl PathJailer {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn safe_join(&self, guest_path: &str) -> Result<PathBuf, SecurityError> {
        // We reject explicit relative traversal markers pre-canonicalization to prevent
        // obscure OS-level bypasses.
        if guest_path.contains("..") || guest_path.contains('\0') {
            return Err(SecurityError::PathTraversalAttempt);
        }

        let full_path = self.root.join(guest_path);
        
        // Canonicalize strictly evaluates symlinks and resolves dots
        // If the file does not exist, canonicalize will fail, so we might need a workaround for creating files.
        // For our tests, this is exact. In production, we evaluate prefixes.
        
        let path_to_eval = if full_path.exists() {
            full_path.canonicalize().unwrap_or(full_path.clone())
        } else {
            // For non-existent files, check the parent
            let parent = full_path.parent().unwrap_or(Path::new(""));
            if parent.exists() {
                parent.canonicalize().unwrap_or(parent.to_path_buf()).join(full_path.file_name().unwrap_or_default())
            } else {
                full_path.clone() // Catch deeper paths in prefix check anyway
            }
        };

        let canon_root = if self.root.exists() {
            self.root.canonicalize().unwrap_or(self.root.clone())
        } else {
            self.root.clone()
        };

        if path_to_eval.starts_with(&canon_root) {
            Ok(path_to_eval)
        } else {
            Err(SecurityError::PathTraversalAttempt)
        }
    }
}

pub struct Watchdog {
    pub start_time: Instant,
    pub time_limit: Duration,
}

impl Watchdog {
    pub fn new(time_limit: Duration) -> Self {
        Self {
            start_time: Instant::now(),
            time_limit,
        }
    }

    pub fn check(&self) -> Result<(), SecurityError> {
        if self.start_time.elapsed() > self.time_limit {
            Err(SecurityError::ResourceExhaustion)
        } else {
            Ok(())
        }
    }
}

/// Defines the business actions expected from an interactive log browsing session.
#[allow(dead_code)]
pub trait ViewerSession {
    /// Returns the current target file, if one has been opened.
    fn log_file(&self) -> Option<&str>;
    /// Starts a forward browsing workflow.
    fn browse(&mut self) -> Result<(), String>;
    /// Searches the current log source by a business keyword.
    fn search(&mut self, keyword: &str) -> Result<(), String>;
    /// Moves the session to a logical byte offset or indexed position.
    fn jump_to(&mut self, offset: u64) -> Result<(), String>;
}

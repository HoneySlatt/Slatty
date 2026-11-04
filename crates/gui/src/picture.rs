//! A picture as the state keeps it, of Slatty's own: a file (covers, key art, icons), or bytes not
//! saved yet (a picture downloaded in Edit game, until saved). The interface turns it into its
//! framework's image (`ui::handle`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Picture {
    File(PathBuf),
    /// The same bytes are the same picture; a copy of them is another.
    Bytes(Arc<[u8]>),
}

impl Picture {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Picture::File(path.into())
    }

    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Self {
        Picture::Bytes(bytes.into())
    }

    /// The file it is shown from, if any.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Picture::File(path) => Some(path),
            Picture::Bytes(_) => None,
        }
    }
}

impl PartialEq for Picture {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Picture::File(a), Picture::File(b)) => a == b,
            (Picture::Bytes(a), Picture::Bytes(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for Picture {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_file_or_the_same_bytes_are_the_same_picture() {
        let file = Picture::from_path("/[FAKE]/cover.jpg");
        assert_eq!(file, Picture::from_path("/[FAKE]/cover.jpg"));
        assert_eq!(file.path(), Some(Path::new("/[FAKE]/cover.jpg")));
        let bytes = Picture::from_bytes(vec![1u8, 2, 3]);
        assert_eq!(bytes, bytes.clone());
        assert_ne!(
            bytes,
            Picture::from_bytes(vec![1u8, 2, 3]),
            "a copy is another"
        );
        assert_ne!(file, bytes);
        assert_eq!(bytes.path(), None);
    }
}

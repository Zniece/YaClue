//! Lazy-load file units. See upstream `cyacas/libyacas/src/deffile.cpp`
//! (`DefFile`/`DefFiles`).
//!
//! `MultiUserFunction.iFileToOpen` points at a `DefFile`; the first call to a
//! function that is declared but not yet defined triggers `InternalUse` of
//! that file (see `evaluator::get_user_function`) — the hook behind the
//! `DefLoad` mechanism.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// A lazy-load file unit: file name + loaded flag + the set of function
/// symbols declared by this file.
#[derive(Clone)]
pub struct DefFile {
    pub file_name: String,
    pub is_loaded: bool,
    pub symbols: HashSet<Rc<str>>,
}

impl DefFile {
    pub fn new(file_name: &str) -> Self {
        DefFile {
            file_name: file_name.to_string(),
            is_loaded: false,
            symbols: HashSet::new(),
        }
    }

    pub fn set_loaded(&mut self) {
        self.is_loaded = true;
    }

    pub fn is_loaded(&self) -> bool {
        self.is_loaded
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }
}

/// Registry of lazy-load files, keyed by file name.
#[derive(Default)]
pub struct DefFiles {
    pub map: HashMap<String, DefFile>,
}

impl DefFiles {
    pub fn file(&mut self, file_name: &str) -> DefFile {
        self.map
            .entry(file_name.to_string())
            .or_insert_with(|| DefFile::new(file_name))
            .clone()
    }
}

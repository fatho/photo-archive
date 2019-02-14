//! Support for asynchronous operations in GTK.

use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct AsyncKey(usize);

impl AsyncKey {
    fn next(self) -> AsyncKey {
        AsyncKey(self.0 + 1)
    }
}

thread_local! {
    static PENDING: RefCell<HashMap<AsyncKey, Box<FnOnce() -> ()>>> = RefCell::new(HashMap::new());

    static NEXT_KEY: RefCell<AsyncKey> = RefCell::new(AsyncKey(0));
}
